// @vitest-environment jsdom
import React from "react";
import { afterEach, it, expect, vi } from "vitest";
import {
  render,
  screen,
  fireEvent,
  waitFor,
  cleanup,
} from "@testing-library/react";
import { VideoPlayer } from "../src/VideoPlayer";
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
const video = {
  title: "Video",
  embedUrl: "https://drive.google.com/file/d/abc/preview",
  playbackUrl: "https://r2.example/video",
  driveReady: false,
  canConfirmDrive: false,
};
it("requires the owner to test the Drive player before confirming", async () => {
  const fetch = vi.fn(async () => ({ ok: true }));
  vi.stubGlobal("fetch", fetch);
  const refresh = vi.fn();
  const { container, rerender } = render(
    <VideoPlayer
      video={{ ...video, canConfirmDrive: true }}
      publicId="id"
      onRefresh={refresh}
    />,
  );
  expect(
    screen.queryByText("Assisti no Drive; confirmar reprodução"),
  ).toBeNull();
  fireEvent.click(screen.getByText("Testar player do Drive"));
  expect(container.querySelector("iframe")?.src).toBe(video.embedUrl);
  fireEvent.click(screen.getByText("Assisti no Drive; confirmar reprodução"));
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce());
  expect(fetch).toHaveBeenCalledOnce();
  expect(fetch).toHaveBeenCalledWith(
    "/api/public/id/drive-ready",
    expect.objectContaining({ method: "POST" }),
  );
  rerender(
    <VideoPlayer
      video={{ ...video, driveReady: true }}
      publicId="id"
      onRefresh={refresh}
    />,
  );
  expect(container.querySelector("video")).toBeNull();
  expect(container.querySelector("iframe")?.getAttribute("src")).toBe(
    video.embedUrl,
  );
});
it("preserves playback position when the signed URL is renewed", () => {
  const { container, rerender } = render(
    <VideoPlayer video={video} publicId="id" onRefresh={() => {}} />,
  );
  const element = container.querySelector("video")!;
  expect(element.getAttribute("src")).toBe(video.playbackUrl);
  expect(container.querySelector("iframe")).toBeNull();
  expect(screen.queryByText("Testar player do Drive")).toBeNull();
  element.currentTime = 42;
  fireEvent.timeUpdate(element);
  rerender(
    <VideoPlayer
      video={{ ...video, playbackUrl: "https://r2.example/renewed" }}
      publicId="id"
      onRefresh={() => {}}
    />,
  );
  expect(container.querySelector("video")).toBe(element);
  expect(element.getAttribute("src")).toBe("https://r2.example/renewed");
  element.currentTime = 0;
  Object.defineProperty(element, "duration", {
    value: 100,
    configurable: true,
  });
  fireEvent.loadedMetadata(element);
  expect(element.currentTime).toBe(42);
});
