import { useEffect, useState } from "react";
import { call, idle, subscribe, type Snapshot } from "./bridge";
export function CaptureBorder() {
  const [snapshot, setSnapshot] = useState(idle);
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    const accept = (value: Snapshot) => { if (!disposed) setSnapshot(previous => value.revision >= previous.revision ? value : previous); };
    void subscribe(accept).then(unlisten => {
      if (disposed) unlisten();
      else { stop = unlisten; void call<Snapshot>("get_snapshot").then(accept).catch(() => {}); }
    }).catch(() => {});
    return () => { disposed = true; stop?.(); };
  }, []);
  return <div className={"capture-outline" + (snapshot.phase === "paused" ? " paused" : "")} aria-hidden="true" />;
}
