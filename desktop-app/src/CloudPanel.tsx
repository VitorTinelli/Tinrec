import { useEffect, useState } from "react";
import { call, message, type RecordingFile } from "./bridge";
interface Upload {
  id: string;
  fileName: string;
  title: string;
  status: string;
  temporaryDone?: boolean;
  temporaryBytes?: number;
  temporaryNote?: string | null;
  uploadedBytes: number;
  sizeBytes: number;
  url: string | null;
  error: string | null;
}
interface Cloud {
  connected: boolean;
  email: string | null;
  connectUrl: string | null;
  code: string | null;
  error: string | null;
  jobs: Upload[];
}
const labels: Record<string, string> = {
  recording: "Gravando · link reservado",
  queued: "Na fila de envio",
  uploading: "Enviando para seu Drive",
  uploaded: "Disponível no site",
  error: "Envio pendente",
  failed: "Gravação interrompida",
};
export function CloudPanel({ files }: { files: RecordingFile[] }) {
  const [cloud, setCloud] = useState<Cloud | null>(null),
    [error, setError] = useState<string | null>(null),
    [pending, setPending] = useState(false),
    [copied, setCopied] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false,
      checking = false;
    const refresh = async () => {
      if (checking) return;
      checking = true;
      try {
        const next = await call<Cloud>("cloud_state");
        if (!disposed) setCloud(next);
      } catch (e) {
        if (!disposed) setError(message(e));
      } finally {
        checking = false;
      }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 1000);
    return () => {
      disposed = true;
      clearInterval(timer);
    };
  }, []);
  async function act(command: string, args?: Record<string, unknown>) {
    setPending(true);
    setError(null);
    try {
      await call(command, args);
      setCloud(await call<Cloud>("cloud_state"));
    } catch (e) {
      setError(message(e));
    } finally {
      setPending(false);
    }
  }
  async function copy(job: Upload) {
    if (!job.url) return;
    try {
      await call("cloud_copy", { id: job.id });
      setCopied(job.id);
    } catch {
      setError(
        "Não foi possível copiar. Selecione o endereço do vídeo abaixo e copie manualmente.",
      );
    }
  }
  const available = files.filter(
    (f) =>
      f.status === "completed" &&
      !cloud?.jobs.some(
        (j) => j.fileName === f.fileName && j.status !== "failed",
      ),
  );
  return (
    <section className="panel cloud-panel" aria-labelledby="cloud-heading">
      <div className="panel-heading">
        <div>
          <h2 id="cloud-heading">Seu Drive, seu link</h2>
          <p>
            {cloud?.connected
              ? `Conectado como ${cloud.email}`
              : "Conecte a mesma conta Google usada no site."}
          </p>
        </div>
        <button
          className="button secondary"
          disabled={pending}
          onClick={() =>
            void act(
              cloud?.connected || cloud?.connectUrl
                ? "cloud_disconnect"
                : "cloud_connect",
            )
          }
        >
          {pending
            ? "Aguarde…"
            : cloud?.connectUrl
              ? "Cancelar conexão"
              : cloud?.connected
                ? "Desconectar"
                : "Conectar Google"}
        </button>
      </div>
      {cloud?.connectUrl ? (
        <div className="notice" role="status">
          <div>
            <strong>Aprove a conexão no navegador</strong>
            <p>
              Confira se o código é o mesmo: <code>{cloud.code}</code>
            </p>
            <p>Se o navegador não abriu, copie este endereço:</p>
            <input
              aria-label="Endereço para conectar o aplicativo"
              readOnly
              value={cloud.connectUrl}
              onFocus={(e) => e.target.select()}
            />
          </div>
        </div>
      ) : null}
      {(error ?? cloud?.error) ? (
        <p className="cloud-error" role="alert">
          {error ?? cloud?.error}
        </p>
      ) : null}
      <p className="cloud-description">
        {cloud?.connected
          ? "Novas gravações terão uma cópia temporária para reprodução enquanto o Google prepara o player do Drive. O link fica disponível durante a gravação e o vídeo abre após o primeiro envio."
          : "Seus vídeos continuam salvos no computador. Ao conectar, novas gravações serão enviadas ao seu Drive e poderão ser vistas por qualquer pessoa com o link."}{" "}
        Mantenha o aplicativo aberto durante o envio. Envios pendentes são
        retomados ao reabrir.
      </p>
      {cloud?.connected ? (
        <button
          className="text-button"
          onClick={() => void act("cloud_open", { id: null })}
          disabled={pending}
        >
          Abrir minha biblioteca no site ↗
        </button>
      ) : null}
      {cloud?.jobs.length ? (
        <div className="cloud-list">
          {cloud.jobs.slice(0, 20).map((job) => (
            <article className="cloud-job" key={job.id}>
              <div className="cloud-job-heading">
                <strong>{job.title}</strong>
                <span role="status">
                  {job.status === "uploading" && !job.temporaryDone
                    ? "Preparando reprodução temporária"
                    : (labels[job.status] ?? job.status)}
                </span>
              </div>
              {job.sizeBytes > 0 ? (
                <progress
                  aria-label={`Envio de ${job.title}`}
                  max={job.sizeBytes}
                  value={
                    job.temporaryDone
                      ? job.uploadedBytes
                      : (job.temporaryBytes ?? 0)
                  }
                />
              ) : null}
              {job.temporaryNote ? <p>{job.temporaryNote}</p> : null}
              {job.error ? <p className="cloud-error">{job.error}</p> : null}
              {job.url ? (
                <>
                  <input
                    className="cloud-link"
                    aria-label={`Link de ${job.title}`}
                    readOnly
                    value={job.url}
                    onFocus={(e) => e.target.select()}
                  />
                  <div className="cloud-actions">
                    <button
                      className="button secondary"
                      onClick={() => void copy(job)}
                    >
                      {copied === job.id ? "Link copiado" : "Copiar link"}
                    </button>
                    <button
                      className="text-button"
                      disabled={pending}
                      onClick={() => void act("cloud_open", { id: job.id })}
                    >
                      Abrir página ↗
                    </button>
                    {job.status === "uploaded" && !job.temporaryDone ? (
                      <button
                        className="text-button"
                        disabled={pending}
                        onClick={() => void act("cloud_retry", { id: job.id })}
                      >
                        Preparar reprodução rápida
                      </button>
                    ) : null}
                    {job.status === "error" ? (
                      <button
                        className="text-button"
                        disabled={pending}
                        onClick={() => void act("cloud_retry", { id: job.id })}
                      >
                        Tentar novamente
                      </button>
                    ) : null}
                  </div>
                </>
              ) : null}
            </article>
          ))}
        </div>
      ) : null}
      {cloud?.connected && available.length ? (
        <div className="cloud-existing">
          <h3>Enviar uma gravação anterior</h3>
          {available.slice(0, 10).map((file) => (
            <div className="cloud-job-heading" key={file.fileName}>
              <span>{file.title}</span>
              <button
                className="button secondary"
                disabled={pending}
                onClick={() =>
                  void act("cloud_enqueue", { fileName: file.fileName })
                }
              >
                Enviar ao Drive
              </button>
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}
