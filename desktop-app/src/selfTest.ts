import { exportPng, loadImage } from "./screenshots/render";
import { call, type Snapshot, type Preview } from "./bridge";

let running: Promise<void> | undefined;
const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms));
async function phase(expected: string): Promise<Snapshot> {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    const snapshot = await call<Snapshot>("get_snapshot");
    if (snapshot.phase === expected) return snapshot;
    if (snapshot.phase === "failed")
      throw new Error(snapshot.error ?? "Falha na captura nativa");
    await sleep(100);
  }
  throw new Error("Tempo limite aguardando " + expected);
}

async function previewFor(
  command: string,
  sessionId?: string | null,
): Promise<Preview> {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    const preview = await call<Preview | null>(command);
    if (preview && (!sessionId || preview.sessionId === sessionId))
      return preview;
    await sleep(100);
  }
  throw new Error("Seletor sem imagem: " + command);
}

async function pickerReady() {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    const value = await call<{ count: number; labels: string[] } | null>(
      "self_test_area_pick",
    );
    if (value) return value;
    await sleep(100);
  }
  throw new Error("Seletor de monitores indisponível");
}
async function borderReady() {
  const deadline = Date.now() + 5000;
  while (!(await call<boolean>("self_test_capture_border"))) {
    if (Date.now() >= deadline)
      throw new Error("Contorno desalinhado ou visibilidade incorreta");
    await sleep(100);
  }
}
async function printReady() {
  const deadline = Date.now() + 10000;
  while (!(await call<boolean>("self_test_print_ready"))) {
    if (Date.now() > deadline)
      throw new Error("O editor do print não ficou pronto");
    await sleep(20);
  }
}
async function nativeController() {
  const base = {
    keepLocal: true,
    title: "Autoteste",
    display: "auto",
    window: "",
    camera: "",
    webcamEnabled: false,
    microphoneDevice: null,
    microphone: false,
    systemAudio: false,
    microphoneVolume: 80,
    systemVolume: 80,
    resolution: 720,
  };
  await call("start_recording", {
    request: { ...base, captureMode: "screen" },
  });
  await phase("recording");
  await sleep(1100);
  await call("control_recording", { action: "pause" });
  await phase("paused");
  await borderReady();
  await sleep(200);
  await call("control_recording", { action: "resume" });
  await phase("recording");
  await sleep(1100);
  await call("control_recording", { action: "stop" });
  const completed = await phase("completed");
  if (!completed.fileName || completed.videoFrames < 1)
    throw new Error("Captura nativa vazia");
  const source = await call<string>("self_test_window", { action: "open" });
  try {
    await call("self_test_shortcuts");
    await call("start_recording", {
      request: { ...base, captureMode: "window", window: source },
    });
    await phase("recording");
    await call("self_test_window", { action: "move" });
    await sleep(250);
    const deadline = Date.now() + 5000;
    while (!(await call<boolean>("self_test_window", { action: "aligned" }))) {
      if (Date.now() >= deadline)
        throw new Error("Overlay não acompanhou a janela capturada");
      await sleep(100);
    }
    await borderReady();
    await sleep(1100);
    await call("control_recording", { action: "stop" });
    await phase("completed");
    await borderReady();
  } finally {
    await call("self_test_window", { action: "close" });
  }
  let monitorCount = 1;
  for (let index = 0; index < monitorCount; index++) {
    const pending = call<import("./bridge").AreaSelection | null>(
      "choose_area",
    );
    const ready = await pickerReady();
    monitorCount = ready.count;
    await call("self_test_area_pick", { index });
    const crop = await pending;
    if (!crop) throw new Error("Seleção de área cancelada");
    await call("start_recording", {
      request: {
        ...base,
        title: "Autoteste area " + index,
        captureMode: "area",
        display: crop.display,
        crop,
      },
    });
    await phase("recording");
    await borderReady();
    await call("control_recording", { action: "pause" });
    await phase("paused");
    await borderReady();
    await call("control_recording", { action: "resume" });
    await phase("recording");
    await sleep(1100);
    await call("control_recording", { action: "stop" });
    await phase("completed");
    await borderReady();
  }
  const cancelled = await call<Snapshot>("start_recording", {
    request: { ...base, captureMode: "area" },
  });
  await previewFor("get_preview", cancelled.sessionId);
  await call("self_test_close_selector", { label: "area" });
  await phase("discarded");
  const area = await call<Snapshot>("start_recording", {
    request: { ...base, captureMode: "area" },
  });
  const preview = await previewFor("get_preview", area.sessionId);
  await call("select_area", {
    sessionId: area.sessionId,
    x: 0,
    y: 0,
    width: Math.min(320, preview.sourceWidth),
    height: Math.min(240, preview.sourceHeight),
  });
  await phase("recording");
  await call("control_recording", { action: "discard" });
  await phase("discarded");
  await call("hide_to_tray");
  const captured = await call<{ image: string }>("capture_print", {
    mode: "screen",
    display: "auto",
    window: "",
  });
  try {
    await call("self_test_print_transition");
    const saved = await call<{ fileName: string }>("export_print", {
      image: captured.image,
      title: "Autoteste print",
      action: "save",
    });
    const files = await call<{ fileName: string }[]>("list_prints");
    if (!files.some((file) => file.fileName === saved.fileName))
      throw new Error("Print salvo não aparece na biblioteca");
  } finally {
    await call("close_print");
  }
  const quick = await call<{ image: string }>("capture_print", {
    mode: "screen",
    display: "auto",
    window: "",
    action: "save",
  });
  if (quick.image !== "")
    throw new Error("A captura direta retornou uma imagem redundante");
  await call("close_print");
  const pendingPrint = call("capture_print", {
    mode: "area",
    display: "auto",
    window: "",
  }).then(
    () => false,
    () => true,
  );
  const readyPrint = await pickerReady();
  await call("self_test_area_pick", { index: readyPrint.count - 1 });
  await previewFor("get_print_area");
  await printReady();
  await call("self_test_print_transition", {
    label: readyPrint.labels[readyPrint.count - 1],
  });
  await call("self_test_close_selector", { label: "print-area" });
  if (!(await pendingPrint))
    throw new Error("Fechar seletor de print não cancelou a captura");
  await call("self_test_print_transition");
  const probe = await call<{ x: number; y: number; rgba: number[] }>(
    "self_test_window",
    {
      action: "probe",
    },
  );
  const finishedArea = call<{ image: string; width: number; height: number }>(
    "capture_print",
    { mode: "area", display: "auto", window: "" },
  ).then(
    (value) => ({ value }),
    (error) => ({ error: String(error) }),
  );
  const readyArea = await pickerReady();
  await call("self_test_area_pick", { index: readyArea.count - 1 });
  const edit = await previewFor("get_print_area");
  await printReady();
  if (edit.image.startsWith("http://tinrec-print.localhost/")) {
    const raw = await loadImage(edit.image);
    const probeCanvas = document.createElement("canvas");
    probeCanvas.width = 1;
    probeCanvas.height = 1;
    const context = probeCanvas.getContext("2d")!;
    context.drawImage(raw, probe.x, probe.y, 1, 1, 0, 0, 1, 1);
    const rgb = context.getImageData(0, 0, 1, 1).data;
    if (
      [36, 104, 172].some(
        (value, index) => Math.abs(value - rgb[index]) > 32,
      ) ||
      probe.rgba.some((value, index) => value !== rgb[index])
    )
      throw new Error(
        `A captura rápida não preservou os pixels da janela de teste: ${Array.from(rgb)} em ${probe.x},${probe.y}`,
      );
  }
  const crop = edit.initialSelection;
  if (!crop) throw new Error("Recorte inicial ausente");
  const image = await exportPng({
    version: 1,
    source: edit.image,
    width: edit.sourceWidth,
    height: edit.sourceHeight,
    operations: [
      {
        id: "crop",
        tool: "crop",
        x: crop.x,
        y: crop.y,
        w: crop.width,
        h: crop.height,
        size: 1,
        color: "#000000",
      },
    ],
  });
  try {
    await call("export_print_selection", {
      request: {
        sessionId: edit.sessionId,
        rect: crop,
        title: "Autoteste recorte direto",
        action: "save",
      },
    });
    await call("export_print", {
      image,
      title: "Autoteste exportacao area",
      action: "save",
    });
    await call("select_print_area", {
      sessionId: edit.sessionId,
      ...crop,
      returnImage: false,
    });
    const result = await finishedArea;
    if (
      !("value" in result) ||
      result.value.image !== "" ||
      result.value.width !== crop.width ||
      result.value.height !== crop.height
    )
      throw new Error(
        "A captura gerou uma imagem redundante ou dimensões incorretas",
      );
  } finally {
    await call("close_print");
  }
}

export function runInstalledTest(): Promise<void> {
  return (running ??= (async () => {
    if (!(await call<boolean>("self_test_mode"))) return;
    let error: string | null = null;
    try {
      await nativeController();
    } catch (reason) {
      error = String(reason);
    }
    await call("desktop_ready", { error });
  })().catch(() => {}));
}
