// The RAM bar between the list and the usage footer: how much of the
// computer's memory is in use, in gigabytes beside the bar. Quiet like the
// footer's bars — amber from 75%, red from 90%. A click opens the system's
// monitor, as the macOS app's does.

import { useEffect, useState } from "preact/hooks";
import { memoryMonitor, memoryNow, type Memory } from "../api";

const GB = 1024 ** 3;
const gigabytes = (bytes: number) =>
  `${(bytes / GB).toLocaleString("pt-BR", { minimumFractionDigits: 1, maximumFractionDigits: 1 })} GB`;

export function MemoryMeter() {
  const [memory, setMemory] = useState<Memory | null>(null);
  useEffect(() => {
    let live = true;
    const read = () =>
      void memoryNow()
        .then((m) => live && setMemory(m))
        .catch(() => live && setMemory(null));
    read();
    const timer = setInterval(read, 3000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, []);
  if (!memory || memory.total === 0) return null;
  const percent = Math.round((memory.used / memory.total) * 100);
  const level = percent >= 90 ? "critical" : percent >= 75 ? "attention" : "";
  return (
    <section
      class="memory-meter"
      aria-label="Memória RAM"
      title={`Memória RAM do computador em uso (${percent}%). Clique para ver os processos.`}
      onClick={() => void memoryMonitor()}
    >
      <div class="memory-title">Memória RAM</div>
      <div class="memory-row">
        <div class="usage-track">
          <div class={`usage-fill ${level}`} style={{ width: `${Math.min(100, percent)}%` }} />
        </div>
        <span class={`memory-figures ${level}`}>
          {gigabytes(memory.used)} / {gigabytes(memory.total)}
        </span>
      </div>
    </section>
  );
}
