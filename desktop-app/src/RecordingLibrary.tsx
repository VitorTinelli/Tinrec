import { useEffect, useRef, useState } from "react";
import {
  MonitorPlay,
  Play,
  Search,
  RefreshCw,
  Video,
  Plus,
  ArrowUpRight,
} from "lucide-react";
import { call, duration, size, message, type RecordingFile } from "./bridge";

// Keep decoding off the UI thread and avoid starting many FFmpeg processes.
let thumbnailQueue = Promise.resolve();
const thumbnails = new Map<string, string>();
function Thumbnail({ file, folder }: { file: RecordingFile; folder: string }) {
  const key = `${folder}/${file.fileName}:${file.modifiedMs}:${file.bytes}`;
  const [image, setImage] = useState<string | undefined>(() =>
    thumbnails.get(key),
  );
  const element = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    let cancelled = false;
    setImage(thumbnails.get(key));
    if (thumbnails.has(key)) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        observer.disconnect();
        thumbnailQueue = thumbnailQueue.then(async () => {
          if (cancelled) return;
          try {
            const value = await call<string>("recording_thumbnail", {
              fileName: file.fileName,
            });
            if (!value || cancelled) return;
            if (thumbnails.size >= 100)
              thumbnails.delete(thumbnails.keys().next().value!);
            thumbnails.set(key, value);
            setImage(value);
          } catch {
            /* A damaged or unsupported file keeps the honest placeholder. */
          }
        });
      },
      { rootMargin: "160px" },
    );
    if (element.current) observer.observe(element.current);
    return () => {
      cancelled = true;
      observer.disconnect();
    };
  }, [key, file.fileName]);
  return (
    <span className="video-thumbnail" ref={element}>
      {image ? (
        <img src={image} alt="" />
      ) : (
        <MonitorPlay size={32} strokeWidth={1.3} />
      )}
      <span className="thumbnail-play">
        <Play size={19} fill="currentColor" />
      </span>
      <span className="thumbnail-duration">
        {file.duration === null ? "MP4" : duration(file.duration)}
      </span>
    </span>
  );
}

export function RecordingLibrary({
  files,
  folder,
  pending,
  loading,
  onOpen,
  onRefresh,
  onRecord,
}: {
  files: RecordingFile[];
  folder: string;
  pending: boolean;
  loading: boolean;
  onOpen: (name: string) => void;
  onRefresh: () => void;
  onRecord: () => void;
}) {
  const [query, setQuery] = useState("");
  const [siteError, setSiteError] = useState<string | null>(null);
  const [openingSite, setOpeningSite] = useState(false);
  const [openingVideo, setOpeningVideo] = useState<string | null>(null);
  async function openSite(fileName?: string) {
    setSiteError(null);
    if (fileName) setOpeningVideo(fileName);
    else setOpeningSite(true);
    try {
      if (fileName) await call("cloud_open_recording", { fileName });
      else await call("cloud_open", { id: null });
    } catch (error) {
      setSiteError(message(error));
    } finally {
      setOpeningSite(false);
      setOpeningVideo(null);
    }
  }
  const filtered = files.filter((file) =>
    file.title
      .toLocaleLowerCase("pt-BR")
      .includes(query.toLocaleLowerCase("pt-BR")),
  );
  return (
    <section className="recording-library" aria-labelledby="library-heading">
      <div className="library-toolbar">
        <h2 id="library-heading">
          Gravações recentes <span className="count-badge">{files.length}</span>
        </h2>
        <button
          className="button secondary library-site"
          disabled={openingSite}
          onClick={() => void openSite()}
        >
          <ArrowUpRight size={15} />
          Ver no site
        </button>
        <label className="library-search">
          <Search size={16} />
          <input
            aria-label="Buscar vídeos"
            placeholder="Buscar vídeos…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            type="search"
          />
        </label>
        <button
          className="icon-button"
          aria-label="Atualizar dispositivos e gravações"
          title="Atualizar"
          onClick={onRefresh}
          disabled={loading}
        >
          <RefreshCw size={16} className={loading ? "spin" : ""} />
        </button>
      </div>
      {siteError ? (
        <p className="notice error" role="alert">
          {siteError}
        </p>
      ) : null}
      {files.length === 0 ? (
        <div className="library-empty">
          <span>
            <Video size={32} strokeWidth={1.3} />
          </span>
          <h3>Uma ideia merece um vídeo.</h3>
          <p>
            Suas gravações aparecerão aqui, prontas para assistir e
            compartilhar.
          </p>
          <button className="button primary" onClick={onRecord}>
            <Plus size={16} />
            Criar meu primeiro vídeo
          </button>
        </div>
      ) : filtered.length === 0 ? (
        <div className="library-empty">
          <Search size={28} />
          <h3>Nenhum vídeo encontrado</h3>
          <p>Tente buscar por outro nome.</p>
        </div>
      ) : (
        <div className="video-grid">
          {filtered.map((file) => (
            <article className="video-card" key={`${folder}/${file.fileName}`}>
              <button
                className="video-card-open"
                onClick={() => onOpen(file.fileName)}
                disabled={pending}
                aria-label={`Abrir ${file.title}`}
              >
                <Thumbnail file={file} folder={folder} />
                <span className="video-card-title" title={file.title}>
                  {file.title}
                </span>
              </button>
              <div className="video-card-meta">
                <time dateTime={new Date(file.modifiedMs).toISOString()}>
                  {new Date(file.modifiedMs).toLocaleDateString("pt-BR", {
                    day: "numeric",
                    month: "short",
                  })}
                </time>
                <span>{size(file.bytes)}</span>
              </div>
              <div className="video-card-actions">
                <button
                  className="text-button"
                  aria-label={`Abrir ${file.title} no site`}
                  disabled={openingSite || openingVideo !== null}
                  onClick={() => void openSite(file.fileName)}
                >
                  <ArrowUpRight size={14} />
                  {openingVideo === file.fileName
                    ? "Abrindo…"
                    : "Abrir no site"}
                </button>
              </div>
              {file.status === "failed" ? (
                <p className="file-warning">Conferir arquivo</p>
              ) : null}
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
