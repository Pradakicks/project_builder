import { beforeEach, describe, expect, it, vi } from "vitest";
import { useToastStore } from "./useToastStore";

describe("useToastStore history ring buffer", () => {
  beforeEach(() => {
    // Reset store state between tests.
    useToastStore.setState({ toasts: [], history: [] });
    vi.useFakeTimers();
  });

  it("records every addToast into history independent of auto-dismiss", () => {
    const { addToast, getHistory } = useToastStore.getState();
    addToast("first", "info");
    addToast("second", "warning");
    addToast("third", "error");

    const history = getHistory();
    expect(history).toHaveLength(3);
    expect(history.map((h) => h.message)).toEqual(["first", "second", "third"]);
    expect(history.map((h) => h.type)).toEqual(["info", "warning", "error"]);

    // Advance past the 4-second auto-dismiss window.
    vi.advanceTimersByTime(5_000);
    // Active toasts are gone…
    expect(useToastStore.getState().toasts).toHaveLength(0);
    // …but history is preserved.
    expect(useToastStore.getState().getHistory()).toHaveLength(3);
  });

  it("caps history at 50 entries, keeping the newest", () => {
    const { addToast, getHistory } = useToastStore.getState();
    for (let i = 0; i < 60; i += 1) {
      addToast(`msg ${i}`, "info");
    }
    const history = getHistory();
    expect(history).toHaveLength(50);
    expect(history[0]?.message).toBe("msg 10");
    expect(history[history.length - 1]?.message).toBe("msg 59");
  });
});
