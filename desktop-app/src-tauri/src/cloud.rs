//! Device credentials stay in the OS credential store. Queue metadata contains no tokens.
use crate::{Reply, lock, now_ms};
use reqwest::{
    Url,
    blocking::{Client, Response},
    header::{CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
const SITE: &str = "https://tinrec.vercel.app";
const CHUNK: usize = 8 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub file_name: String,
    pub title: String,
    path: PathBuf,
    account: String,
    resolution: u32,
    duration_ms: u64,
    pub status: String,
    pub uploaded_bytes: u64,
    pub size_bytes: u64,
    pub url: Option<String>,
    #[serde(default)]
    pub temporary_done: bool,
    #[serde(default)]
    pub temporary_bytes: u64,
    #[serde(default)]
    pub temporary_note: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    remove_local_after_upload: bool,
    attempts: u32,
    next_retry: u64,
}
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Saved {
    account: Option<String>,
    email: Option<String>,
    jobs: Vec<Job>,
}
#[derive(Clone)]
struct Auth {
    token: String,
    account: String,
}
#[derive(Clone)]
struct Pending {
    code: String,
    secret: String,
    url: String,
    expires: u64,
}
struct Inner {
    saved: Saved,
    auth: Option<Auth>,
    pending: Option<Pending>,
    error: Option<String>,
    poll_at: u64,
}
pub struct Cloud {
    inner: Mutex<Inner>,
    operation: Mutex<()>,
    file: PathBuf,
    credential: String,
    client: Client,
    site: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub email: Option<String>,
    pub connected: bool,
    pub connect_url: Option<String>,
    pub code: Option<String>,
    pub error: Option<String>,
    pub jobs: Vec<Job>,
}
impl Cloud {
    pub fn new(data: &Path) -> Reply<Arc<Self>> {
        let file = data.join("uploads.json");
        let mut saved: Saved = match fs::read(&file) {
            Ok(b) => serde_json::from_slice(&b).map_err(|_| {
                "Fila de envios inválida; preserve uploads.json para recuperação.".to_string()
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Saved::default(),
            Err(e) => return Err(e.to_string()),
        };
        for job in &mut saved.jobs {
            if job.status == "recording" {
                job.status = "failed".into();
                job.error =
                    Some("Gravação interrompida. Confira o arquivo local antes de enviar.".into());
            }
            if job.status == "uploading" {
                job.status = "queued".into();
            }
        }
        let credential = format!("{:x}", Sha256::digest(data.to_string_lossy().as_bytes()));
        let mut error = None;
        let auth = if let Some(account) = saved.account.clone() {
            match keyring::Entry::new("com.tinelli.tinrec", &credential)
                .and_then(|e| e.get_password())
            {
                Ok(token) => Some(Auth { token, account }),
                Err(_) => {
                    error=Some("Conecte sua conta Google para retomar os envios. Desbloqueie o chaveiro do sistema se necessário.".into());
                    None
                }
            }
        } else {
            None
        };
        let cloud = Arc::new(Self {
            operation: Mutex::new(()),
            inner: Mutex::new(Inner {
                saved,
                auth,
                pending: None,
                error,
                poll_at: 0,
            }),
            file,
            credential,
            site: SITE.into(),
            client: Client::builder()
                .redirect(Policy::none())
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(90))
                .build()
                .map_err(|_| "Não foi possível iniciar a conexão HTTPS".to_string())?,
        });
        cloud.persist(&*lock(&cloud.inner)?)?;
        Ok(cloud)
    }
    fn persist(&self, inner: &Inner) -> Reply<()> {
        let bytes = serde_json::to_vec_pretty(&inner.saved).map_err(|e| e.to_string())?;
        let temp = self.file.with_extension("tmp");
        fs::write(&temp, bytes).map_err(|e| e.to_string())?;
        // Atomic replace on each supported platform (std rename replaces files on Windows too).
        fs::rename(temp, &self.file).map_err(|e| e.to_string())
    }
    pub fn view(&self) -> Reply<View> {
        let i = lock(&self.inner)?;
        Ok(View {
            email: i.saved.email.clone(),
            connected: i.auth.is_some(),
            connect_url: i.pending.as_ref().map(|p| p.url.clone()),
            code: i.pending.as_ref().map(|p| p.code.clone()),
            error: i.error.clone(),
            jobs: i
                .saved
                .jobs
                .iter()
                .filter(|j| Some(&j.account) == i.saved.account.as_ref() && j.status != "discarded")
                .cloned()
                .rev()
                .collect(),
        })
    }
    fn api(&self, path: &str, auth: Option<&Auth>, body: Option<Value>) -> Reply<Value> {
        let url = format!("{}/api{path}", self.site);
        let mut req = if body.is_some() {
            self.client.post(url)
        } else {
            self.client.get(url)
        };
        if let Some(a) = auth {
            req = req.bearer_auth(&a.token)
        }
        if let Some(b) = body {
            req = req.json(&b)
        }
        let r = req.send().map_err(|_| {
            "Sem conexão com o site. O envio será retomado automaticamente.".to_string()
        })?;
        let status = r.status();
        let value: Value = r.json().unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(value["error"]["message"]
                .as_str()
                .or_else(|| value["message"].as_str())
                .unwrap_or(if status.as_u16() == 401 {
                    "Sessão expirada. Conecte sua conta Google novamente."
                } else {
                    "O site não conseguiu concluir a operação. Tente novamente."
                })
                .into());
        }
        Ok(value)
    }
    pub fn recording_url(&self, path: &Path) -> Reply<String> {
        self.reserved_url(path)?.ok_or(
            "O link deste vídeo ainda está sendo preparado. Tente novamente em instantes.".into(),
        )
    }
    pub fn reserved_url(&self, path: &Path) -> Reply<Option<String>> {
        let i = lock(&self.inner)?;
        let account = &i
            .auth
            .as_ref()
            .ok_or("Conecte sua conta Google em Configurações para abrir este vídeo no site.")?
            .account;
        let job = i
            .saved
            .jobs
            .iter()
            .rev()
            .find(|job| {
                &job.account == account
                    && job.status != "discarded"
                    && (job.path == path
                        || fs::canonicalize(&job.path).is_ok_and(|value| value == path))
            })
            .ok_or("Este vídeo ainda não foi enviado ao site pela conta conectada.")?;
        Ok(job.url.clone())
    }
    pub fn connect(&self) -> Reply<String> {
        let _operation = lock(&self.operation)?;
        let i = lock(&self.inner)?;
        if i.auth.is_some() {
            return Err("Desconecte a conta atual antes de conectar outra.".into());
        }
        drop(i);
        let v = self.api("/devices/start", None, Some(json!({})))?;
        let code = field(&v, "code")?;
        let secret = field(&v, "secret")?;
        let url = field(&v, "url")?;
        let u = Url::parse(&url).map_err(|_| "Link de conexão inválido".to_string())?;
        if u.origin().ascii_serialization() != SITE || u.path() != "/connect" {
            return Err("Link de conexão inválido".into());
        }
        let mut i = lock(&self.inner)?;
        i.pending = Some(Pending {
            code,
            secret,
            url: url.clone(),
            expires: now_ms() + 600_000,
        });
        i.error = None;
        Ok(url)
    }
    pub fn disconnect(&self) -> Reply<()> {
        let _operation = lock(&self.operation)?;
        // Same lock as each upload chunk: no upload can continue after this returns.
        let auth = lock(&self.inner)?.auth.clone();
        match keyring::Entry::new("com.tinelli.tinrec", &self.credential)
            .and_then(|e| e.delete_credential())
        {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => return Err(
                "Não foi possível remover a sessão do chaveiro. Desbloqueie-o e tente novamente."
                    .into(),
            ),
        }
        let mut i = lock(&self.inner)?;
        i.auth = None;
        i.pending = None;
        i.saved.account = None;
        i.saved.email = None;
        i.error = None;
        self.persist(&i)?;
        drop(i);
        if let Some(a) = auth {
            let _ = self.api("/devices/revoke", Some(&a), Some(json!({})));
        }
        Ok(())
    }
    pub fn enqueue(
        &self,
        id: String,
        path: PathBuf,
        title: String,
        resolution: u32,
        duration: f64,
        recording: bool,
    ) -> Reply<()> {
        let mut i = lock(&self.inner)?;
        let Some(auth) = i.auth.clone() else {
            return Err("Conecte sua conta Google para enviar o vídeo.".into());
        };
        if i.saved
            .jobs
            .iter()
            .any(|j| j.path == path && j.account == auth.account && j.status != "failed")
        {
            return Ok(());
        }
        let image = path.extension().is_some_and(|extension| extension == "png");
        i.saved.jobs.push(Job {
            id,
            file_name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            path,
            title,
            account: auth.account,
            resolution,
            duration_ms: (duration * 1000.).round() as u64,
            status: if recording { "recording" } else { "queued" }.into(),
            uploaded_bytes: 0,
            size_bytes: 0,
            url: None,
            temporary_done: image,
            temporary_bytes: 0,
            temporary_note: None,
            error: None,
            remove_local_after_upload: false,
            attempts: 0,
            next_retry: 0,
        });
        self.persist(&i)
    }
    pub fn remove_local_after_upload(&self, id: &str) -> Reply<()> {
        let mut i = lock(&self.inner)?;
        let job = i
            .saved
            .jobs
            .iter_mut()
            .find(|job| job.id == id)
            .ok_or("Envio não encontrado")?;
        job.remove_local_after_upload = true;
        self.persist(&i)
    }
    pub fn finish(&self, id: &str, duration: f64, success: bool) -> Reply<()> {
        let mut i = lock(&self.inner)?;
        if let Some(j) = i.saved.jobs.iter_mut().find(|j| j.id == id) {
            j.duration_ms = (duration * 1000.).round() as u64;
            j.status = if success { "queued" } else { "failed" }.into();
            if !success {
                j.error = Some("A gravação não foi concluída; arquivo local preservado.".into());
            }
        }
        self.persist(&i)
    }
    pub fn discard(&self, id: &str) -> Reply<()> {
        let mut i = lock(&self.inner)?;
        if let Some(job) = i.saved.jobs.iter_mut().find(|j| j.id == id) {
            if !["recording", "discarded"].contains(&job.status.as_str()) {
                return Err("Este vídeo já entrou na fila de envio e não pode ser descartado pelos controles de gravação.".into());
            }
            job.status = "discarded".into();
            job.error = None;
            job.next_retry = 0;
        }
        self.persist(&i)
    }
    pub fn retry(&self, id: &str) -> Reply<()> {
        let mut i = lock(&self.inner)?;
        let account = i
            .auth
            .as_ref()
            .ok_or("Conecte sua conta Google")?
            .account
            .clone();
        let j = i
            .saved
            .jobs
            .iter_mut()
            .find(|j| j.id == id && j.account == account)
            .ok_or("Envio não encontrado")?;
        if ["recording", "failed", "discarded"].contains(&j.status.as_str()) {
            return Err("Este vídeo não está disponível para repetir o envio.".into());
        }
        if j.status == "uploaded" {
            j.status = "queued".into();
            j.temporary_done = false;
            j.temporary_bytes = 0;
        }
        j.next_retry = 0;
        j.error = None;
        self.persist(&i)
    }
    pub fn start(self: &Arc<Self>) {
        let cloud = self.clone();
        std::thread::spawn(move || {
            loop {
                cloud.tick();
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }
    fn tick(&self) {
        let Ok(_operation) = self.operation.lock() else {
            return;
        };
        let Ok(mut i) = self.inner.lock() else { return };
        if let Some(p) = i.pending.clone() {
            if now_ms() > p.expires {
                i.pending = None;
                i.error = Some("Código expirado. Inicie uma nova conexão.".into());
                return;
            }
            if now_ms() < i.poll_at {
                return;
            }
            i.poll_at = now_ms() + 3000;
            drop(i);
            let response = self.api(
                "/devices/poll",
                None,
                Some(json!({"code":p.code,"secret":p.secret})),
            );
            match response {
                Ok(v) if v["token"].is_string() => {
                    let token = v["token"].as_str().unwrap().to_owned();
                    let a = Auth {
                        token,
                        account: String::new(),
                    };
                    let account=self.api("/me",Some(&a),None).and_then(|me| {
                        let account=field(&me,"id")?;
                        let email=field(&me,"email")?;
                        keyring::Entry::new("com.tinelli.tinrec",&self.credential)
                            .and_then(|e|e.set_password(&a.token))
                            .map_err(|_|"Não foi possível salvar a sessão no chaveiro do sistema. Desbloqueie o chaveiro e conecte novamente.".to_string())?;
                        Ok((account,email))
                    });
                    if account.is_err() {
                        let _ = self.api("/devices/revoke", Some(&a), Some(json!({})));
                    }
                    let Ok(mut i) = self.inner.lock() else { return };
                    match account {
                        Ok((account, email)) => {
                            i.saved.account = Some(account.clone());
                            i.saved.email = Some(email);
                            i.auth = Some(Auth {
                                account,
                                token: a.token,
                            });
                            i.error = None;
                        }
                        Err(e) => i.error = Some(e),
                    }
                    i.pending = None;
                    let _ = self.persist(&i);
                }
                Ok(_) => {}
                Err(e) => {
                    if let Ok(mut i) = self.inner.lock() {
                        i.error = Some(e);
                    }
                }
            }
            return;
        }
        let Some(a) = i.auth.clone() else { return };
        let Some(index) = i.saved.jobs.iter().position(|j| {
            j.account == a.account
                && j.next_retry <= now_ms()
                && ((j.status == "uploaded" && j.remove_local_after_upload)
                    || (j.status == "recording" && j.url.is_none())
                    || ["queued", "uploading", "error", "failed", "discarded"]
                        .contains(&j.status.as_str()))
        }) else {
            return;
        };
        let mut job = i.saved.jobs[index].clone();
        let was_recording = job.status == "recording";
        drop(i);
        // One bounded chunk per tick. Never hold a file or its contents in the WebView.
        let result = self.step(&a, &mut job);
        if let Err(error) = result {
            job.error = Some(error);
            job.attempts = job.attempts.saturating_add(1);
            job.next_retry =
                now_ms() + (10_000u64.saturating_mul(1u64 << job.attempts.min(5))).min(300_000);
            if !["recording", "failed", "discarded", "uploaded"].contains(&job.status.as_str()) {
                job.status = "error".into();
            }
        }
        let Ok(mut i) = self.inner.lock() else { return };
        if was_recording && i.saved.jobs[index].status != "recording" {
            job.status = i.saved.jobs[index].status.clone();
            job.duration_ms = i.saved.jobs[index].duration_ms;
            job.error = i.saved.jobs[index].error.clone();
        }
        if job.status == "discarded" && job.next_retry == u64::MAX {
            i.saved.jobs.remove(index);
        } else {
            i.saved.jobs[index] = job;
        }
        if let Err(e) = self.persist(&i) {
            i.error = Some(format!("Não foi possível salvar a fila de envios: {e}"));
        }
    }
    fn step(&self, a: &Auth, j: &mut Job) -> Reply<()> {
        if j.status == "uploaded" && j.remove_local_after_upload {
            let pending = self
                .file
                .parent()
                .ok_or("Pasta de dados indisponível")?
                .join("pending-recordings");
            let pending_prints = pending.with_file_name("pending-prints");
            if j.path.parent() != Some(pending.as_path())
                && j.path.parent() != Some(pending_prints.as_path())
            {
                return Err("Limpeza restrita à pasta temporária do Tinrec.".into());
            }
            crate::discard_files(&j.path)?;
            j.remove_local_after_upload = false;
            j.error = None;
            return Ok(());
        }
        let base = format!("/recordings/{}", j.id);
        if j.status == "discarded" {
            // A reservation can have reached the server even when its response was lost.
            self.api(&format!("{base}/discard"), Some(a), Some(json!({})))?;
            j.next_retry = u64::MAX;
            return Ok(());
        }
        if j.status == "failed" {
            if j.url.is_some() {
                self.api(
                    &format!("{base}/error"),
                    Some(a),
                    Some(json!({"code":"recording_failed"})),
                )?;
            }
            j.next_retry = u64::MAX;
            return Ok(());
        }
        if j.url.is_none() {
            let v = self.api(
                "/recordings",
                Some(a),
                Some(json!({"id":j.id,"title":j.title,"resolution":j.resolution,"mediaType":if j.path.extension().is_some_and(|extension|extension=="png"){"image"}else{"video"}})),
            )?;
            let url = field(&v, "url")?;
            if !url.starts_with(&format!("{SITE}/v/")) {
                return Err("Link do vídeo inválido".into());
            }
            j.url = Some(url);
            j.error = None;
            return Ok(());
        }
        if j.status == "recording" {
            return Ok(());
        }
        let meta = fs::symlink_metadata(&j.path).map_err(|_| {
            "Arquivo local não encontrado. Restaure-o e tente novamente.".to_string()
        })?;
        if !meta.is_file() || meta.len() == 0 || j.path.to_string_lossy().ends_with(".partial.mp4")
        {
            return Err("Arquivo local inválido; o envio foi interrompido.".into());
        }
        if j.size_bytes != 0 && j.size_bytes != meta.len() {
            return Err("O arquivo local mudou desde o início do envio.".into());
        }
        j.size_bytes = meta.len();
        if !j.temporary_done {
            let v = self.api(
                &format!("{base}/temporary"),
                Some(a),
                Some(json!({"sizeBytes":j.size_bytes,"durationMs":j.duration_ms})),
            )?;
            if v["completed"] == true || v["available"] == false {
                j.temporary_done = true;
                j.temporary_note = if v["available"] == false {
                    Some(
                        match v["reason"].as_str() {
                            Some("size_limit") => "Vídeo acima de 2 GiB: envio somente ao Drive.",
                            Some("capacity") => {
                                "Armazenamento temporário cheio: envio somente ao Drive."
                            }
                            _ => "Cópia temporária indisponível: envio somente ao Drive.",
                        }
                        .into(),
                    )
                } else {
                    None
                };
                j.uploaded_bytes = 0;
                j.error = None;
                return Ok(());
            }
            if v["retry"] == true {
                return Ok(());
            }
            let uri = field(&v, "url")?;
            self.validate_temporary_upload(&uri)?;
            let offset = v["offset"]
                .as_u64()
                .ok_or("Resposta de envio temporário inválida")?;
            let length = v["length"]
                .as_u64()
                .ok_or("Resposta de envio temporário inválida")?;
            if length == 0
                || length > CHUNK as u64
                || offset
                    .checked_add(length)
                    .is_none_or(|end| end > j.size_bytes)
            {
                return Err("Bloco temporário fora dos limites do arquivo".into());
            }
            let mut file = File::open(&j.path)
                .map_err(|_| "Não foi possível abrir o vídeo local".to_string())?;
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| e.to_string())?;
            let mut bytes = vec![0; length as usize];
            file.read_exact(&mut bytes)
                .map_err(|_| "Não foi possível ler o vídeo local".to_string())?;
            let r = self.client.put(uri).body(bytes).send().map_err(|_| {
                "Conexão interrompida durante o envio temporário. Tentaremos novamente.".to_string()
            })?;
            if !r.status().is_success() {
                return Err(format!(
                    "O R2 recusou o envio temporário (HTTP {}).",
                    r.status().as_u16()
                ));
            }
            j.temporary_bytes = offset + length;
            j.error = None;
            j.attempts = 0;
            j.next_retry = 0;
            j.status = "uploading".into();
            return Ok(());
        }
        let mut session = self.api(
            &format!("{base}/upload"),
            Some(a),
            Some(json!({"sizeBytes":j.size_bytes,"durationMs":j.duration_ms})),
        )?;
        if session["completed"] == true {
            j.status = "uploaded".into();
            j.uploaded_bytes = j.size_bytes;
            j.error = None;
            return Ok(());
        }
        let mut uri = field(&session, "uri")?;
        self.validate_upload(&uri)?;
        let mut response = self.probe(&uri, j.size_bytes)?;
        if [404, 410].contains(&response.status().as_u16()) {
            session = self.api(
                &format!("{base}/upload"),
                Some(a),
                Some(json!({"sizeBytes":j.size_bytes,"durationMs":j.duration_ms,"restart":true})),
            )?;
            uri = field(&session, "uri")?;
            self.validate_upload(&uri)?;
            response = self.probe(&uri, j.size_bytes)?;
        }
        if response.status().is_success() {
            return self.complete(a, j, &base);
        }
        let offset = offset(&response, j.size_bytes)?;
        if offset >= j.size_bytes {
            return Err("O Drive ainda não confirmou o arquivo. Tentaremos novamente.".into());
        }
        let mut file = File::open(&j.path)
            .map_err(|_| "Não foi possível abrir a gravação local".to_string())?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let count = (j.size_bytes - offset).min(CHUNK as u64) as usize;
        let mut bytes = vec![0; count];
        file.read_exact(&mut bytes)
            .map_err(|_| "Não foi possível ler a gravação local".to_string())?;
        let end = offset + count as u64;
        let r = self
            .client
            .put(&uri)
            .header(
                CONTENT_TYPE,
                if j.path
                    .extension()
                    .is_some_and(|extension| extension == "png")
                {
                    "image/png"
                } else {
                    "video/mp4"
                },
            )
            .header(
                CONTENT_RANGE,
                format!("bytes {}-{}/{}", offset, end - 1, j.size_bytes),
            )
            .body(bytes)
            .send()
            .map_err(|_| {
                "Conexão interrompida. O envio será retomado do ponto confirmado pelo Drive."
                    .to_string()
            })?;
        if r.status().is_success() {
            if end != j.size_bytes {
                return Err("O Drive confirmou um envio incompleto.".into());
            }
            return self.complete(a, j, &base);
        }
        let confirmed = offset_from_response(&r, j.size_bytes)?;
        if confirmed <= offset || confirmed > end {
            return Err("O Drive retornou um progresso inesperado.".into());
        }
        j.uploaded_bytes = confirmed;
        j.status = "uploading".into();
        j.error = None;
        j.attempts = 0;
        j.next_retry = 0;
        let _ = self.api(
            &format!("{base}/progress"),
            Some(a),
            Some(json!({"uploadedBytes":confirmed})),
        );
        Ok(())
    }
    fn validate_temporary_upload(&self, uri: &str) -> Reply<()> {
        #[cfg(test)]
        if self.site != SITE && uri.starts_with(&format!("{}/upload/", self.site)) {
            return Ok(());
        }
        validate_temporary_uri(uri)
    }
    fn validate_upload(&self, uri: &str) -> Reply<()> {
        #[cfg(test)]
        if self.site != SITE && uri.starts_with(&format!("{}/upload/", self.site)) {
            return Ok(());
        }
        validate_uri(uri)
    }
    fn probe(&self, uri: &str, total: u64) -> Reply<Response> {
        self.client
            .put(uri)
            .header(CONTENT_LENGTH, 0)
            .header(CONTENT_RANGE, format!("bytes */{total}"))
            .body(Vec::new())
            .send()
            .map_err(|_| "Sem conexão com o Drive. Tentaremos novamente.".into())
    }
    fn complete(&self, a: &Auth, j: &mut Job, base: &str) -> Reply<()> {
        self.api(&format!("{base}/complete"), Some(a), Some(json!({})))?;
        j.status = "uploaded".into();
        j.uploaded_bytes = j.size_bytes;
        j.error = None;
        j.attempts = 0;
        Ok(())
    }
}
fn field(v: &Value, key: &str) -> Reply<String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Resposta incompleta do site".into())
}
fn validate_temporary_uri(uri: &str) -> Reply<()> {
    let u = Url::parse(uri).map_err(|_| "Endereço temporário inválido".to_string())?;
    if u.scheme() != "https"
        || u.host_str() != Some("c17dc8288b6500f944ef1f9f762b8b1f.r2.cloudflarestorage.com")
        || u.port_or_known_default() != Some(443)
        || !u.username().is_empty()
        || u.password().is_some()
        || !u.path().starts_with("/tinrec/temporary/")
    {
        return Err("Destino temporário não autorizado".into());
    }
    Ok(())
}

fn validate_uri(uri: &str) -> Reply<()> {
    let u = Url::parse(uri).map_err(|_| "Endereço de upload inválido".to_string())?;
    if u.scheme() != "https"
        || u.host_str() != Some("www.googleapis.com")
        || u.port_or_known_default() != Some(443)
        || !u.username().is_empty()
        || u.password().is_some()
        || !u.path().starts_with("/upload/drive/")
    {
        return Err("Endereço de upload não autorizado".into());
    }
    Ok(())
}
fn offset(r: &Response, total: u64) -> Reply<u64> {
    offset_from_response(r, total)
}
fn offset_from_response(r: &Response, total: u64) -> Reply<u64> {
    if r.status().as_u16() != 308 {
        return Err(format!(
            "O Drive recusou o envio (HTTP {}). Tente novamente ou reconecte sua conta.",
            r.status().as_u16()
        ));
    }
    parse_range(r.headers().get(RANGE).and_then(|h| h.to_str().ok()), total)
}
fn parse_range(range: Option<&str>, total: u64) -> Reply<u64> {
    match range {
        None => Ok(0),
        Some(v) => v
            .strip_prefix("bytes=0-")
            .and_then(|s| s.parse::<u64>().ok())
            .and_then(|n| n.checked_add(1))
            .filter(|n| *n <= total)
            .ok_or_else(|| "Progresso inválido retornado pelo Drive".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    // One bounded HTTP reader for all upload scenarios. EOF must fail immediately.
    fn read_test_request(
        listener: &std::net::TcpListener,
        path: &str,
    ) -> (
        std::io::BufReader<std::net::TcpStream>,
        Vec<u8>,
        Option<String>,
    ) {
        use std::io::{BufRead, BufReader};
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let (stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "mock request timeout: {path}"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("{error}"),
            }
        };
        // Windows may inherit the listener's nonblocking mode on accepted sockets.
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "missing request: {path}"
        );
        assert!(line.starts_with(path), "{line} != {path}");
        let mut length = 0;
        let mut range = None;
        loop {
            assert!(
                std::time::Instant::now() < deadline,
                "mock headers timeout: {path}"
            );
            line.clear();
            assert!(
                reader.read_line(&mut line).unwrap() > 0,
                "truncated headers: {path}"
            );
            if line == "\r\n" {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(value) = lower.strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().unwrap();
            }
            if let Some(value) = lower.strip_prefix("content-range:") {
                range = Some(value.trim().to_string());
            }
        }
        assert!(length <= CHUNK, "unexpected body size: {length}");
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes).unwrap();
        (reader, bytes, range)
    }
    #[test]
    fn restrict_upload_destination() {
        assert!(
            validate_uri("https://www.googleapis.com/upload/drive/v3/files?upload_id=x").is_ok()
        );
        for u in [
            "http://www.googleapis.com/upload/drive/v3/files",
            "https://evil.test/upload/drive/v3/files",
            "https://www.googleapis.com.evil.test/upload/drive/v3/files",
            "https://user@www.googleapis.com/upload/drive/v3/files",
            "https://www.googleapis.com/drive/v3/files",
        ] {
            assert!(validate_uri(u).is_err());
        }
    }
    #[test]
    fn drive_range_is_bounded() {
        assert_eq!(parse_range(None, 100).unwrap(), 0);
        assert_eq!(parse_range(Some("bytes=0-49"), 100).unwrap(), 50);
        for r in [
            "bytes=1-49",
            "bytes=0-100",
            "bytes=0-18446744073709551615",
            "bad",
        ] {
            assert!(parse_range(Some(r), 100).is_err());
        }
    }
    fn test_cloud() -> (Arc<Cloud>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("tinrec-cloud-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        (Cloud::new(&dir).unwrap(), dir)
    }
    fn authenticate(c: &Cloud, account: &str) {
        let mut i = c.inner.lock().unwrap();
        i.auth = Some(Auth {
            token: "secret-test-token".into(),
            account: account.into(),
        });
        i.saved.account = Some(account.into());
    }
    #[test]
    fn temporary_print_is_removed_only_after_upload_and_local_print_is_protected() {
        let (c, dir) = test_cloud();
        authenticate(&c, "first");
        let pending = dir.join("pending-prints");
        fs::create_dir_all(&pending).unwrap();
        let path = pending.join("test.png");
        fs::write(&path, b"png").unwrap();
        c.enqueue("print".into(), path.clone(), "test".into(), 1080, 0., false)
            .unwrap();
        c.remove_local_after_upload("print").unwrap();
        let auth = c.inner.lock().unwrap().auth.clone().unwrap();
        let mut job = c.inner.lock().unwrap().saved.jobs[0].clone();
        assert!(path.exists());
        job.status = "uploaded".into();
        c.step(&auth, &mut job).unwrap();
        assert!(!path.exists());
        let local = dir.join("local.png");
        fs::write(&local, b"png").unwrap();
        job.path = local.clone();
        job.remove_local_after_upload = true;
        assert!(c.step(&auth, &mut job).is_err());
        assert!(local.exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn temporary_local_copy_is_preserved_until_confirmed_and_policy_survives_restart() {
        let (c, dir) = test_cloud();
        authenticate(&c, "first");
        let pending = dir.join("pending-recordings");
        fs::create_dir_all(&pending).unwrap();
        let path = pending.join("test.mp4");
        fs::write(&path, b"video").unwrap();
        let report = tinrec_capture::encoder::sidecar(&path, ".capture.json");
        fs::write(&report, b"{}").unwrap();
        c.enqueue("id".into(), path.clone(), "test".into(), 1080, 0., true)
            .unwrap();
        c.remove_local_after_upload("id").unwrap();
        let saved: Saved =
            serde_json::from_slice(&fs::read(dir.join("uploads.json")).unwrap()).unwrap();
        assert!(saved.jobs[0].remove_local_after_upload);
        let auth = c.inner.lock().unwrap().auth.clone().unwrap();
        let mut job = saved.jobs[0].clone();
        job.status = "failed".into();
        c.step(&auth, &mut job).unwrap();
        assert!(path.exists());
        assert!(report.exists());
        job.status = "uploaded".into();
        c.step(&auth, &mut job).unwrap();
        assert!(!path.exists());
        assert!(!report.exists());
        assert!(!job.remove_local_after_upload);
        // Repeating cleanup after a crash between removal and persistence is safe.
        job.remove_local_after_upload = true;
        c.step(&auth, &mut job).unwrap();
        let kept = dir.join("kept.mp4");
        fs::write(&kept, b"keep").unwrap();
        job.path = kept.clone();
        job.remove_local_after_upload = true;
        assert!(c.step(&auth, &mut job).is_err());
        assert!(kept.exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn recording_links_match_the_file_and_connected_account() {
        let (c, dir) = test_cloud();
        authenticate(&c, "first");
        let first = dir.join("one").join("video.mp4");
        let second = dir.join("two").join("video.mp4");
        for path in [&first, &second] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"video").unwrap();
        }
        c.enqueue("one".into(), first.clone(), "video".into(), 1080, 1., false)
            .unwrap();
        c.enqueue(
            "two".into(),
            second.clone(),
            "video".into(),
            1080,
            1.,
            false,
        )
        .unwrap();
        {
            let mut i = c.inner.lock().unwrap();
            i.saved.jobs[0].url = Some(format!("{SITE}/v/one"));
            i.saved.jobs[1].url = Some(format!("{SITE}/v/two"));
        }
        assert_eq!(c.recording_url(&first).unwrap(), format!("{SITE}/v/one"));
        assert_eq!(c.recording_url(&second).unwrap(), format!("{SITE}/v/two"));
        authenticate(&c, "another");
        assert!(c.recording_url(&first).is_err());
        c.inner.lock().unwrap().auth = None;
        assert!(c.recording_url(&first).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn print_page_waits_for_reservation_and_reuses_existing_upload() {
        let (c, dir) = test_cloud();
        authenticate(&c, "owner");
        let path = dir.join("pending-prints").join("print.png");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"png").unwrap();
        c.enqueue(
            "print".into(),
            path.clone(),
            "Print".into(),
            1080,
            0.,
            false,
        )
        .unwrap();
        assert_eq!(c.reserved_url(&path).unwrap(), None);
        c.enqueue(
            "duplicate".into(),
            path.clone(),
            "Print".into(),
            1080,
            0.,
            false,
        )
        .unwrap();
        {
            let mut i = c.inner.lock().unwrap();
            assert_eq!(i.saved.jobs.len(), 1);
            i.saved.jobs[0].url = Some(format!("{SITE}/v/print"));
        }
        // Temporary files can be removed after upload before the opener runs.
        fs::remove_file(&path).unwrap();
        assert_eq!(
            c.reserved_url(&path).unwrap(),
            Some(format!("{SITE}/v/print"))
        );
        authenticate(&c, "another");
        assert!(c.reserved_url(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn edited_png_queue_skips_video_processing_after_restart() {
        let (cloud, dir) = test_cloud();
        authenticate(&cloud, "owner");
        let path = dir.join("edited.png");
        fs::write(&path, b"edited-png").unwrap();
        cloud
            .enqueue(
                "print".into(),
                path.clone(),
                "Print editado".into(),
                1080,
                0.,
                false,
            )
            .unwrap();
        let saved: Saved = serde_json::from_slice(&fs::read(&cloud.file).unwrap()).unwrap();
        assert!(saved.jobs[0].temporary_done);
        assert_eq!(saved.jobs[0].path, path);
        assert_eq!(saved.jobs[0].status, "queued");
        assert_eq!(saved.jobs[0].duration_ms, 0);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn queue_survives_restart_and_isolates_accounts() {
        let (c, dir) = test_cloud();
        authenticate(&c, "first");
        c.enqueue(
            "id".into(),
            dir.join("test.mp4"),
            "Teste".into(),
            720,
            1.0,
            false,
        )
        .unwrap();
        let persisted = fs::read_to_string(&c.file).unwrap();
        assert!(!persisted.contains("secret-test-token"));
        // Avoid reading an OS credential during this test; account scope belongs to each job.
        {
            let mut i = c.inner.lock().unwrap();
            i.saved.account = None;
            c.persist(&i).unwrap();
        }
        let loaded = Cloud::new(&dir).unwrap();
        authenticate(&loaded, "second");
        assert!(loaded.view().unwrap().jobs.is_empty());
        assert!(loaded.retry("id").is_err());
        authenticate(&loaded, "first");
        assert_eq!(loaded.view().unwrap().jobs.len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn resumes_chunks_after_restart_and_finalizes_once() {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let base = origin.clone();
        let worker = std::thread::spawn(move || {
            let total = CHUNK + 6;
            // A previously uploaded 3-byte prefix is acknowledged by the first probe.
            let steps = vec![
                (
                    "POST /api/recordings ".to_string(),
                    None,
                    200,
                    format!(r#"{{"url":"{SITE}/v/test"}}"#),
                    String::new(),
                ),
                (
                    "POST /api/recordings/id/upload ".into(),
                    None,
                    200,
                    format!(r#"{{"uri":"{base}/upload/session"}}"#),
                    String::new(),
                ),
                (
                    "PUT /upload/session ".into(),
                    Some(format!("bytes */{total}")),
                    308,
                    String::new(),
                    "Range: bytes=0-2\r\n".into(),
                ),
                (
                    "PUT /upload/session ".into(),
                    Some(format!("bytes 3-{}/{total}", CHUNK + 2)),
                    308,
                    String::new(),
                    format!("Range: bytes=0-{}\r\n", CHUNK + 2),
                ),
                (
                    "POST /api/recordings/id/progress ".into(),
                    None,
                    200,
                    "{}".into(),
                    String::new(),
                ),
                (
                    "POST /api/recordings/id/upload ".into(),
                    None,
                    200,
                    format!(r#"{{"uri":"{base}/upload/session"}}"#),
                    String::new(),
                ),
                (
                    "PUT /upload/session ".into(),
                    Some(format!("bytes */{total}")),
                    308,
                    String::new(),
                    format!("Range: bytes=0-{}\r\n", CHUNK + 2),
                ),
                (
                    "PUT /upload/session ".into(),
                    Some(format!("bytes {}-{}/{total}", CHUNK + 3, total - 1)),
                    200,
                    "{}".into(),
                    String::new(),
                ),
                (
                    "POST /api/recordings/id/complete ".into(),
                    None,
                    200,
                    "{}".into(),
                    String::new(),
                ),
            ];
            listener.set_nonblocking(true).unwrap();
            for (path, range, status, body, extra) in steps {
                let (mut reader, contents, received_range) = read_test_request(&listener, &path);
                assert_eq!(received_range, range);
                if path.starts_with("PUT") && !contents.is_empty() {
                    assert!(contents.iter().all(|b| *b == 42));
                }
                let reply = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\n{extra}Connection: close\r\n\r\n{body}",
                    body.len()
                );
                reader.get_mut().write_all(reply.as_bytes()).unwrap();
            }
        });
        let (cloud, dir) = test_cloud();
        let mut cloud = Arc::try_unwrap(cloud).ok().unwrap();
        cloud.site = origin;
        authenticate(&cloud, "first");
        let path = dir.join("test.mp4");
        fs::write(&path, vec![42; CHUNK + 6]).unwrap();
        cloud
            .enqueue("id".into(), path, "Teste".into(), 720, 1., false)
            .unwrap();
        let a = cloud.inner.lock().unwrap().auth.clone().unwrap();
        let mut j = cloud.inner.lock().unwrap().saved.jobs[0].clone();
        j.temporary_done = true;
        cloud.step(&a, &mut j).unwrap();
        cloud.step(&a, &mut j).unwrap();
        assert_eq!(j.uploaded_bytes, (CHUNK + 3) as u64);
        {
            let mut i = cloud.inner.lock().unwrap();
            i.saved.jobs[0] = j;
            i.saved.account = None;
            cloud.persist(&i).unwrap();
        }
        let restarted = Cloud::new(&dir).unwrap();
        let mut restarted = Arc::try_unwrap(restarted).ok().unwrap();
        restarted.site = cloud.site.clone();
        let mut j = restarted.inner.lock().unwrap().saved.jobs[0].clone();
        assert_eq!(j.status, "queued");
        restarted.step(&a, &mut j).unwrap();
        assert_eq!(j.status, "uploaded");
        assert_eq!(j.uploaded_bytes, j.size_bytes);
        worker.join().unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn temporary_destination_is_restricted() {
        assert!(validate_temporary_uri("https://c17dc8288b6500f944ef1f9f762b8b1f.r2.cloudflarestorage.com/tinrec/temporary/video.mp4?signature=x").is_ok());
        for uri in [
            "http://c17dc8288b6500f944ef1f9f762b8b1f.r2.cloudflarestorage.com/tinrec/temporary/v.mp4",
            "https://evil.test/tinrec/temporary/video.mp4",
            "https://c17dc8288b6500f944ef1f9f762b8b1f.r2.cloudflarestorage.com/other/video.mp4",
        ] {
            assert!(validate_temporary_uri(uri).is_err());
        }
    }
    #[test]
    fn temporary_upload_survives_restart_before_drive_stage() {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let site = format!("http://{}", listener.local_addr().unwrap());
        let origin = site.clone();
        let worker = std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let steps = [
                (
                    "POST /api/recordings/id/temporary ",
                    format!(r#"{{"url":"{origin}/upload/part","offset":0,"length":10}}"#),
                ),
                ("PUT /upload/part ", "{}".into()),
                (
                    "POST /api/recordings/id/temporary ",
                    r#"{"completed":true}"#.into(),
                ),
                (
                    "POST /api/recordings/id/upload ",
                    r#"{"completed":true}"#.into(),
                ),
            ];
            for (path, body) in steps {
                let (mut reader, bytes, _) = read_test_request(&listener, path);
                assert!(bytes.len() < 1000);
                if path.starts_with("PUT") {
                    assert_eq!(bytes, vec![42; 10]);
                }
                reader.get_mut().write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).unwrap();
            }
        });
        let (c, dir) = test_cloud();
        let mut c = Arc::try_unwrap(c).ok().unwrap();
        c.site = site.clone();
        authenticate(&c, "first");
        let path = dir.join("test.mp4");
        fs::write(&path, [42; 10]).unwrap();
        c.enqueue("id".into(), path, "Teste".into(), 720, 1., false)
            .unwrap();
        let auth = c.inner.lock().unwrap().auth.clone().unwrap();
        let mut j = c.inner.lock().unwrap().saved.jobs[0].clone();
        j.url = Some(format!("{SITE}/v/test"));
        c.step(&auth, &mut j).unwrap();
        assert_eq!(j.temporary_bytes, 10);
        assert!(!j.temporary_done);
        assert_eq!(j.uploaded_bytes, 0);
        {
            let mut i = c.inner.lock().unwrap();
            i.saved.jobs[0] = j;
            i.saved.account = None;
            c.persist(&i).unwrap();
        }
        let restarted = Cloud::new(&dir).unwrap();
        let mut restarted = Arc::try_unwrap(restarted).ok().unwrap();
        restarted.site = site;
        let mut j = restarted.inner.lock().unwrap().saved.jobs[0].clone();
        restarted.step(&auth, &mut j).unwrap();
        assert!(j.temporary_done);
        restarted.step(&auth, &mut j).unwrap();
        assert_eq!(j.status, "uploaded");
        worker.join().unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn discarded_capture_never_enters_upload_even_if_its_file_is_missing() {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let site = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut reader, body, _) =
                read_test_request(&listener, "POST /api/recordings/id/discard ");
            assert_eq!(body, b"{}");
            reader
                .get_mut()
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .unwrap();
        });
        let (c, dir) = test_cloud();
        let mut c = Arc::try_unwrap(c).ok().unwrap();
        c.site = site;
        authenticate(&c, "first");
        c.enqueue(
            "id".into(),
            dir.join("missing.mp4"),
            "Teste".into(),
            720,
            0.,
            true,
        )
        .unwrap();
        c.discard("id").unwrap();
        assert!(c.view().unwrap().jobs.is_empty());
        c.tick();
        assert!(c.inner.lock().unwrap().saved.jobs.is_empty());
        assert!(c.retry("id").is_err());
        worker.join().unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
