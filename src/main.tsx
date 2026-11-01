import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Video,
  Grid2X2,
  Settings,
  ArrowUpRight,
  Search,
  Plus,
  Monitor,
  Link2,
  Check,
  Play,
  LoaderCircle,
  ArrowLeft,
  LogOut,
  HardDrive,
  ShieldCheck,
  Download,
  MoreHorizontal,
  Clock,
  AlertCircle,
  Mic,
  ChevronRight,
  X,
} from "lucide-react";
import "./style.css";
import { VideoPlayer, type Playback } from "./VideoPlayer";

type Recording = {
  id: string;
  publicId: string;
  title: string;
  createdAt: string;
  durationMs: number;
  resolution: number;
  status: string;
  sizeBytes: number;
  uploadedBytes: number;
  errorCode: string | null;
  url: string;
};
type ApiError = Error & { status?: number };
async function api<T>(path: string, options: RequestInit = {}): Promise<T> {
  const r = await fetch("/api" + path, {
    ...options,
    headers: { "Content-Type": "application/json", ...options.headers },
  });
  const b = await r.json();
  if (!r.ok)
    throw Object.assign(new Error(b.message ?? "Não foi possível carregar."), {
      status: r.status,
    });
  return b;
}
const time = (ms: number) =>
  `${Math.floor(ms / 60000)}:${Math.floor((ms / 1000) % 60)
    .toString()
    .padStart(2, "0")}`;
const date = (s: string) =>
  new Intl.DateTimeFormat("pt-BR", { day: "2-digit", month: "short" }).format(
    new Date(s),
  );
function Brand() {
  return (
    <a className="brand" href="/">
      <span className="brand-mark">
        <span />
      </span>
      tinrec<span className="brand-dot">.</span>
    </a>
  );
}
function Toast({ text }: { text: string }) {
  return text ? (
    <div className="toast" role="status">
      <Check size={17} />
      {text}
    </div>
  ) : null;
}
function App() {
  const [ready, setReady] = useState<boolean | null>(null),
    [user, setUser] = useState<{ email: string } | null>(null),
    [error, setError] = useState("");
  useEffect(() => {
    api<{ configured: boolean }>("/health")
      .then(async (h) => {
        setReady(h.configured);
        if (h.configured) {
          try {
            setUser(await api("/me"));
          } catch (e) {
            if ((e as ApiError).status !== 401) setError((e as Error).message);
          }
        }
      })
      .catch((e) => setError(e.message));
  }, []);
  if (location.pathname.startsWith("/v/"))
    return <PublicVideo id={location.pathname.split("/")[2]} />;
  if (ready === null && !error)
    return (
      <div className="full-center">
        <LoaderCircle className="spin" />
        <p>Preparando seu espaço…</p>
      </div>
    );
  if (location.pathname === "/connect" && ready) return <Connect user={user} />;
  return <Dashboard configured={!!ready} user={user} error={error} />;
}
function Dashboard({
  configured,
  user,
  error: initialError,
}: {
  configured: boolean;
  user: { email: string } | null;
  error: string;
}) {
  const [videos, setVideos] = useState<Recording[]>([]),
    [query, setQuery] = useState(""),
    [section, setSection] = useState("library"),
    [error, setError] = useState(initialError),
    [loading, setLoading] = useState(!!user),
    [toast, setToast] = useState(""),
    [modal, setModal] = useState(false),
    [rename, setRename] = useState<Recording | null>(null),
    [draft, setDraft] = useState(""),
    [saving, setSaving] = useState(false);
  useEffect(() => {
    if (!user) return;
    let active = true;
    async function load() {
      try {
        const items = await api<Recording[]>("/recordings");
        if (active) {
          setVideos(items);
          setError("");
        }
      } catch (e) {
        if (active) setError((e as Error).message);
      } finally {
        if (active) setLoading(false);
      }
    }
    void load();
    const t = setInterval(load, 2500);
    return () => {
      active = false;
      clearInterval(t);
    };
  }, [user]);
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(""), 3500);
    return () => clearTimeout(t);
  }, [toast]);
  async function copy(url: string) {
    try {
      await navigator.clipboard.writeText(url);
      setToast("Link copiado. Pronto para compartilhar.");
    } catch {
      setError("Não foi possível copiar. Abra o vídeo e copie o endereço.");
    }
  }
  const filtered = videos.filter((v) =>
    v.title.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
  );
  async function saveTitle(e: React.FormEvent) {
    e.preventDefault();
    if (!rename) return;
    setSaving(true);
    try {
      await api("/recordings/" + rename.id, {
        method: "PATCH",
        body: JSON.stringify({ title: draft }),
      });
      setVideos((v) =>
        v.map((i) => (i.id === rename.id ? { ...i, title: draft } : i)),
      );
      setRename(null);
      setToast("Título atualizado.");
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  }
  return (
    <div className="shell">
      <aside className="sidebar">
        <Brand />
        <div className="workspace-label">SEU ESPAÇO</div>
        <nav>
          <button
            className={section === "library" ? "nav active" : "nav"}
            onClick={() => setSection("library")}
          >
            <Grid2X2 size={19} />
            Minha biblioteca<span>{videos.length}</span>
          </button>
          <button
            className={section === "settings" ? "nav active" : "nav"}
            onClick={() => setSection("settings")}
          >
            <Settings size={19} />
            Configurações
          </button>
        </nav>
        <div className="sidebar-bottom">
          <div className="drive-box">
            <span className="drive-icon">
              <HardDrive size={19} />
            </span>
            <div>
              <strong>Seu vídeo, seu Drive</strong>
              <p>Armazenamento na sua conta.</p>
            </div>
            {user && <span className="online-dot" />}
          </div>
          <div className="profile">
            <span className="avatar">
              {user?.email.slice(0, 1).toUpperCase() ?? "T"}
            </span>
            <div>
              <strong>{user ? "Meu espaço" : "Bem-vindo ao Tinrec"}</strong>
              <p>{user?.email ?? "Grave e compartilhe ideias"}</p>
            </div>
            {user && (
              <button
                className="icon-button"
                aria-label="Sair"
                onClick={async () => {
                  await api("/logout", { method: "POST" });
                  location.reload();
                }}
              >
                <LogOut size={17} />
              </button>
            )}
          </div>
        </div>
      </aside>
      <main>
        <header className="topbar">
          <div className="breadcrumb">
            Meu espaço <ChevronRight size={14} />
            <span>
              {section === "library" ? "Biblioteca" : "Configurações"}
            </span>
          </div>
          <a
            className="subtle-link"
            href="#"
            onClick={(e) => {
              e.preventDefault();
              setModal(true);
            }}
          >
            <Monitor size={16} />
            Aplicativo Windows
            <ArrowUpRight size={14} />
          </a>
        </header>
        <div className="content">
          <div className="page-title">
            <div>
              <div className="eyebrow">MENOS REUNIÕES. MAIS CLAREZA.</div>
              <h1>
                {section === "library"
                  ? "Minha biblioteca"
                  : "Seu espaço, do seu jeito"}
              </h1>
              <p>
                {section === "library"
                  ? "Suas ideias em vídeo. Prontas para chegar mais longe."
                  : "Conecte sua conta e prepare sua próxima gravação."}
              </p>
            </div>
            <button className="primary" onClick={() => setModal(true)}>
              <Plus size={18} />
              Nova gravação
            </button>
          </div>
          {error && (
            <div className="notice error" role="alert">
              <AlertCircle size={18} />
              {error}
              <button
                className="icon-button"
                onClick={() => setError("")}
                aria-label="Fechar aviso"
              >
                <X size={16} />
              </button>
            </div>
          )}
          {!configured && (
            <div className="notice">
              <Settings size={18} />
              <div>
                <strong>Seu Tinrec está quase pronto.</strong> A conexão com
                Google Drive e o acesso à conta aguardam configuração.
              </div>
              <span className="pill">Configuração inicial</span>
            </div>
          )}
          {configured && !user && (
            <div className="notice">
              <ShieldCheck size={19} />
              <div>
                <strong>Sua biblioteca é privada.</strong> Entre para ver e
                gerenciar suas gravações.
              </div>
              <a className="primary small" href="/api/auth/google">
                Entrar com Google
                <ArrowUpRight size={15} />
              </a>
            </div>
          )}
          {section === "settings" ? (
            <div className="settings-grid">
              <article className="panel">
                <span className="feature-icon">
                  <HardDrive />
                </span>
                <h2>Google Drive</h2>
                <p>
                  Os vídeos ficam na pasta Tinrec da sua conta. Apenas os
                  arquivos compartilhados podem ser vistos por quem tem o link.
                </p>
                <div className="setting-row">
                  <span>Conta conectada</span>
                  <strong>{user?.email ?? "Ainda não conectada"}</strong>
                </div>
                {configured && (
                  <a className="secondary" href="/api/auth/google">
                    {user ? "Reconectar Google" : "Conectar Google"}
                  </a>
                )}
              </article>
              <article className="panel">
                <span className="feature-icon">
                  <Monitor />
                </span>
                <h2>Preferências de gravação</h2>
                <p>
                  Escolha o nome padrão, a resolução e os dispositivos de áudio
                  diretamente no aplicativo Windows. Suas preferências são
                  salvas neste computador.
                </p>
                <button className="secondary" onClick={() => setModal(true)}>
                  Abrir instruções
                  <ArrowUpRight size={16} />
                </button>
              </article>
            </div>
          ) : (
            <>
              <section className="hero">
                <div className="hero-copy">
                  <span className="hero-tag">
                    <span />
                    DO SEU DESKTOP PARA QUALQUER PESSOA
                  </span>
                  <h2>
                    Mostre. Explique.
                    <br />
                    <span>Compartilhe em um link.</span>
                  </h2>
                  <p>
                    Capture a tela, dê voz à sua ideia e deixe o resto com o
                    Tinrec. Simples assim.
                  </p>
                  <button className="secondary" onClick={() => setModal(true)}>
                    <Monitor size={17} />
                    Começar a gravar
                    <ArrowUpRight size={16} />
                  </button>
                  <div className="hero-foot">
                    <ShieldCheck size={14} />
                    Salvo no seu Google Drive <span>•</span> 720p ou 1080p
                  </div>
                </div>
                <div className="hero-art" aria-hidden="true">
                  <div className="art-grid" />
                  <div className="mini-window">
                    <div className="window-bar">
                      <i />
                      <i />
                      <i />
                      <span>Uma ideia bem explicada</span>
                    </div>
                    <div className="window-inner">
                      <div className="mock-sidebar">
                        <b />
                        <b />
                        <b />
                        <b />
                      </div>
                      <div className="mock-doc">
                        <span />
                        <strong />
                        <em />
                        <em />
                        <div className="mock-cards">
                          <i />
                          <i />
                          <i />
                        </div>
                      </div>
                      <div className="play-orb">
                        <Play fill="currentColor" size={22} />
                      </div>
                    </div>
                    <div className="record-toolbar">
                      <span className="record-dot" />
                      <b>02:34</b>
                      <div />
                      <Mic size={16} />
                      <Monitor size={16} />
                      <span className="stop-square" />
                    </div>
                  </div>
                  <div className="floating-link">
                    <span>
                      <Link2 size={18} />
                    </span>
                    <div>
                      <strong>Link pronto para compartilhar</strong>
                      <p>Sua próxima conversa começa aqui.</p>
                    </div>
                    <Check size={17} />
                  </div>
                </div>
              </section>
              <div className="library-toolbar">
                <div className="tabs">
                  <button className="selected">
                    Todos os vídeos <span>{videos.length}</span>
                  </button>
                </div>
                <label className="search">
                  <Search size={17} />
                  <input
                    aria-label="Buscar vídeos"
                    placeholder="Buscar um vídeo…"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                  />
                </label>
                <span className="sort-label">Mais recentes primeiro</span>
              </div>
              {loading ? (
                <div className="empty">
                  <LoaderCircle className="spin" />
                  <h3>Carregando sua biblioteca…</h3>
                </div>
              ) : filtered.length ? (
                <div className="video-grid">
                  {filtered.map((v) => (
                    <article className="video-card" key={v.id}>
                      <a className="video-preview" href={"/v/" + v.publicId}>
                        <div className="preview-lines">
                          <span />
                          <span />
                          <span />
                        </div>
                        <span className="preview-play">
                          <Play size={21} fill="currentColor" />
                        </span>
                        <span className="quality">{v.resolution}p</span>
                        <span className="duration">{time(v.durationMs)}</span>
                      </a>
                      <div className="card-body">
                        <div className="card-title">
                          <a href={"/v/" + v.publicId}>{v.title}</a>
                          <button
                            className="icon-button"
                            aria-label={"Renomear " + v.title}
                            onClick={() => {
                              setRename(v);
                              setDraft(v.title);
                            }}
                          >
                            <MoreHorizontal size={19} />
                          </button>
                        </div>
                        <div className="card-meta">
                          <span>{date(v.createdAt)}</span>
                          <span className={"status " + v.status}>
                            {v.status === "uploaded"
                              ? "No Google Drive"
                              : v.status === "uploading"
                                ? `Enviando · ${v.sizeBytes ? Math.floor((v.uploadedBytes / v.sizeBytes) * 100) : 0}%`
                                : v.status === "error"
                                  ? "Precisa de atenção"
                                  : "Gravando"}
                          </span>
                        </div>
                        <button
                          className="copy-button"
                          onClick={() => copy(v.url)}
                        >
                          <Link2 size={15} />
                          Copiar link
                          <ArrowUpRight size={14} />
                        </button>
                      </div>
                    </article>
                  ))}
                </div>
              ) : (
                <div className="empty">
                  <span className="empty-icon">
                    <Video size={28} />
                    <span>+</span>
                  </span>
                  <h3>
                    {query
                      ? "Nenhum vídeo encontrado"
                      : "Sua primeira ideia merece um vídeo"}
                  </h3>
                  <p>
                    {query
                      ? "Tente buscar por outro título."
                      : "Faça uma gravação no aplicativo. Ela aparecerá aqui, pronta para compartilhar."}
                  </p>
                  {!query && (
                    <button
                      className="text-button"
                      onClick={() => setModal(true)}
                    >
                      Criar minha primeira gravação
                      <ArrowUpRight size={16} />
                    </button>
                  )}
                </div>
              )}
              <div className="library-footer">
                <ShieldCheck size={14} />
                Só você vê esta biblioteca. Seus clientes acessam apenas o vídeo
                compartilhado.<span>TINREC · FEITO PARA SER SIMPLES</span>
              </div>
            </>
          )}
        </div>
      </main>
      <Toast text={toast} />
      {modal && (
        <Modal
          title="Sua próxima gravação começa no desktop"
          close={() => setModal(false)}
        >
          <p>
            Abra o Tinrec no Windows, conecte sua conta e escolha a tela inteira
            ou uma área.
          </p>
          <ol className="steps">
            <li>
              <span>1</span>Escolha 720p ou 1080p e os dispositivos de áudio.
            </li>
            <li>
              <span>2</span>Dê um nome ao vídeo e inicie a gravação.
            </li>
            <li>
              <span>3</span>Ao parar, o link é copiado. O envio continua pelo
              aplicativo.
            </li>
          </ol>
          <div className="notice">
            <Download size={20} />
            <p>
              O instalador é distribuído junto com o projeto. Se já instalou,
              abra <strong>Tinrec</strong> pelo menu Iniciar.
            </p>
          </div>
          <button className="primary" onClick={() => setModal(false)}>
            Entendi
            <Check size={16} />
          </button>
        </Modal>
      )}
      {rename && (
        <Modal title="Renomear vídeo" close={() => setRename(null)}>
          <form onSubmit={saveTitle}>
            <label className="form-label">
              Título
              <input
                autoFocus
                required
                maxLength={180}
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
              />
            </label>
            <button disabled={saving || !draft.trim()} className="primary">
              {saving ? "Salvando…" : "Salvar título"}
            </button>
          </form>
        </Modal>
      )}
    </div>
  );
}
function Modal({
  title,
  close,
  children,
}: {
  title: string;
  close: () => void;
  children: React.ReactNode;
}) {
  useEffect(() => {
    const f = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    document.addEventListener("keydown", f);
    return () => document.removeEventListener("keydown", f);
  }, [close]);
  return (
    <div className="modal-backdrop" onClick={close}>
      <section
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onClick={(e) => e.stopPropagation()}
      >
        <button
          className="icon-button close"
          aria-label="Fechar"
          onClick={close}
        >
          <X size={20} />
        </button>
        <h2>{title}</h2>
        {children}
      </section>
    </div>
  );
}
function Connect({ user }: { user: { email: string } | null }) {
  const [code, setCode] = useState(
      new URLSearchParams(location.search).get("code") ??
        sessionStorage.getItem("tinrec_code") ??
        "",
    ),
    [done, setDone] = useState(false),
    [error, setError] = useState("");
  useEffect(() => {
    if (code) sessionStorage.setItem("tinrec_code", code);
  }, [code]);
  return (
    <div className="connect-page">
      <Brand />
      <section className="panel">
        <span className="feature-icon">
          <Monitor />
        </span>
        <h1>{done ? "Aplicativo conectado" : "Conectar o Tinrec"}</h1>
        {done ? (
          <p>Volte ao aplicativo Windows para começar a gravar.</p>
        ) : (
          <>
            <p>
              Aprove somente o código exibido no seu aplicativo. Isso permite
              enviar gravações para sua biblioteca.
            </p>
            {!user ? (
              <a className="primary" href="/api/auth/google">
                Entrar com Google
              </a>
            ) : (
              <form
                onSubmit={async (e) => {
                  e.preventDefault();
                  try {
                    await api("/devices/approve", {
                      method: "POST",
                      body: JSON.stringify({ code }),
                    });
                    sessionStorage.removeItem("tinrec_code");
                    setDone(true);
                  } catch (e) {
                    setError((e as Error).message);
                  }
                }}
              >
                <label className="form-label">
                  Código do aplicativo
                  <input
                    required
                    value={code}
                    onChange={(e) => setCode(e.target.value)}
                    autoComplete="off"
                  />
                </label>
                <button className="primary">
                  Conectar aplicativo
                  <ShieldCheck size={17} />
                </button>
              </form>
            )}
          </>
        )}
        {error && (
          <p role="alert" className="error-text">
            {error}
          </p>
        )}
      </section>
      <a className="subtle-link" href="/">
        Voltar para a biblioteca
      </a>
    </div>
  );
}
function PublicVideo({ id }: { id: string }) {
  const [video, setVideo] = useState<{
      title: string;
      status: string;
      progress: number;
      embedUrl: string | null;
      playbackUrl?: string | null;
      playbackExpiresAt?: number | null;
      driveReady?: boolean;
      canConfirmDrive?: boolean;
    } | null>(null),
    [error, setError] = useState("");
  const [refresh, setRefresh] = useState(0);
  useEffect(() => {
    let active = true;
    const get = async () => {
      try {
        const v = await api<NonNullable<typeof video>>(
          "/public/" + encodeURIComponent(id),
        );
        if (active) {
          setVideo((previous) =>
            previous?.playbackUrl &&
            v.playbackUrl &&
            previous.playbackExpiresAt &&
            previous.playbackExpiresAt > Date.now() + 120_000 &&
            previous.driveReady === v.driveReady
              ? {
                  ...v,
                  playbackUrl: previous.playbackUrl,
                  playbackExpiresAt: previous.playbackExpiresAt,
                }
              : v,
          );
          setError("");
          document.title = v.title + " — Tinrec";
        }
      } catch (e) {
        if (active) setError((e as Error).message);
      }
    };
    void get();
    const t = setInterval(get, 2000);
    return () => {
      active = false;
      clearInterval(t);
    };
  }, [id, refresh]);
  return (
    <div className="public-page">
      <header>
        <Brand />
        <span>Uma ideia. Um vídeo. Uma conversa.</span>
      </header>
      <main>
        <div className="eyebrow">COMPARTILHADO COM VOCÊ</div>
        <h1>{video?.title ?? "Seu vídeo está aqui"}</h1>
        {!error && (video?.playbackUrl || video?.embedUrl) ? (
          <VideoPlayer
            video={video as Playback}
            publicId={id}
            onRefresh={() => {
              setVideo(null);
              setRefresh((n) => n + 1);
            }}
          />
        ) : (
          <div className="player">
            {error ? (
              <div className="full-center">
                <AlertCircle />
                <h2>Vídeo indisponível</h2>
                <p>{error}</p>
              </div>
            ) : video?.embedUrl ? (
              <iframe
                src={video.embedUrl}
                title={video.title}
                allow="autoplay; fullscreen"
                allowFullScreen
              />
            ) : (
              <div className="full-center">
                {video?.status === "error" ? (
                  <AlertCircle size={34} />
                ) : (
                  <LoaderCircle size={34} className="spin" />
                )}
                <h2>
                  {video?.status === "error"
                    ? "O envio precisa de atenção"
                    : "Seu vídeo está sendo preparado"}
                </h2>
                <p>
                  {video?.status === "error"
                    ? "O autor precisa retomar o envio pelo aplicativo."
                    : "Você pode deixar esta página aberta. O vídeo aparecerá aqui após o envio."}
                </p>
                {video?.status === "uploading" && (
                  <>
                    <progress max={100} value={video.progress} />
                    <span>{video.progress}% enviado</span>
                  </>
                )}
              </div>
            )}
          </div>
        )}
        <div className="player-note">
          <HardDrive size={17} />
          <p>
            {video?.playbackUrl
              ? "O link do Tinrec permanece o mesmo durante a troca para o Drive."
              : video?.embedUrl
                ? "O Google Drive pode levar alguns minutos para processar o vídeo. Se o player ainda não reproduzir, tente novamente em instantes."
                : "Armazenado no Google Drive do autor. Compartilhado com Tinrec."}
          </p>
        </div>
      </main>
      <footer>
        Conversas mais claras começam com <strong>tinrec.</strong>
      </footer>
    </div>
  );
}
// Restore the explicit device-approval page after Google OAuth returns.
if (
  location.pathname === "/" &&
  new URLSearchParams(location.search).has("signedin") &&
  sessionStorage.getItem("tinrec_code")
)
  location.replace("/connect");
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
