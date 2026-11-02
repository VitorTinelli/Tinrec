// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { RecordingLibrary } from "./RecordingLibrary";
import { call } from "./bridge";
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  call: vi.fn(),
}));
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      constructor(
        private callback: (entries: { isIntersecting: boolean }[]) => void,
      ) {}
      observe() {
        this.callback([{ isIntersecting: true }]);
      }
      disconnect() {}
    },
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
it("busca vídeos, abre o arquivo correto e carrega as miniaturas retornadas pelo backend", async () => {
  vi.mocked(call).mockResolvedValue("data:image/jpeg;base64,dGVzdA==");
  const onOpen = vi.fn(),
    user = userEvent.setup();
  render(
    <RecordingLibrary
      files={[
        {
          fileName: "demo.mp4",
          title: "Demonstração",
          modifiedMs: 1000,
          bytes: 1000000,
          duration: 35,
          status: "completed",
        },
        {
          fileName: "reuniao.mp4",
          title: "Reunião",
          modifiedMs: 2000,
          bytes: 2000000,
          duration: 62,
          status: "completed",
        },
      ]}
      folder="/tmp/test-library"
      pending={false}
      loading={false}
      onOpen={onOpen}
      onRefresh={vi.fn()}
      onRecord={vi.fn()}
    />,
  );
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith("recording_thumbnail", {
      fileName: "demo.mp4",
    }),
  );
  for (const title of ["Demonstração", "Reunião"]) {
    await waitFor(() =>
      expect(
        screen
          .getByRole("button", { name: `Abrir ${title}` })
          .querySelector("img")
          ?.getAttribute("src"),
      ).toBe("data:image/jpeg;base64,dGVzdA=="),
    );
  }
  expect(screen.getByText("00:35")).not.toBeNull();
  await user.click(screen.getByRole("button", { name: "Ver no site" }));
  expect(call).toHaveBeenCalledWith("cloud_open", { id: null });
  await user.type(screen.getByRole("searchbox"), "REUNIÃO");
  expect(
    screen.queryByRole("button", { name: "Abrir Demonstração" }),
  ).toBeNull();
  await user.click(screen.getByRole("button", { name: "Abrir Reunião" }));
  expect(onOpen).toHaveBeenCalledWith("reuniao.mp4");
  await user.click(
    screen.getByRole("button", { name: "Abrir Reunião no site" }),
  );
  expect(call).toHaveBeenCalledWith("cloud_open_recording", {
    fileName: "reuniao.mp4",
  });
  await user.clear(screen.getByRole("searchbox"));
  await user.type(screen.getByRole("searchbox"), "inexistente");
  expect(screen.getByText("Nenhum vídeo encontrado")).not.toBeNull();
});
