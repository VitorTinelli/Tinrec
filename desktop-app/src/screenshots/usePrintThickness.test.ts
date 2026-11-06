// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { usePrintThickness } from "./usePrintThickness";
afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.restoreAllMocks();
});
it("keeps thickness across editors and remounts and respects each editor's limits", () => {
  const overlay = renderHook(() => usePrintThickness(2, 12));
  act(() => {
    overlay.result.current[1]((value) => value + 1);
    overlay.result.current[1]((value) => value + 1);
  });
  expect(overlay.result.current[0]).toBe(6);
  overlay.unmount();
  const editor = renderHook(() => usePrintThickness(1, 500));
  expect(editor.result.current[0]).toBe(6);
  act(() => editor.result.current[1](10));
  editor.unmount();
  const reopened = renderHook(() => usePrintThickness(2, 12));
  expect(reopened.result.current[0]).toBe(10);
  act(() => reopened.result.current[1](99));
  expect(reopened.result.current[0]).toBe(12);
});
it("accepts invalid stored values and unavailable storage without breaking editing", () => {
  localStorage.setItem("tinrec.print.thickness.v1", "invalid");
  const editor = renderHook(() => usePrintThickness(2, 12));
  expect(editor.result.current[0]).toBe(4);
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("unavailable");
  });
  act(() => editor.result.current[1](7));
  expect(editor.result.current[0]).toBe(7);
});
