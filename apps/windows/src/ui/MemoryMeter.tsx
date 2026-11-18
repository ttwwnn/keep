// The RAM bar between the list and the usage footer: how much of the
// computer's memory is in use, in gigabytes beside the bar. Quiet like the
// footer's bars — amber from 75%, red from 90%. A click opens the system's
// monitor, as the macOS app's does.

import { signal } from "@preact/signals";
import { useEffect } from "preact/hooks";
import { memoryMonitor, memoryNow, type Memory } from "../api";

const GB = 1024 ** 3;
const gigabytes = (bytes: number) =>
  `${(bytes / GB).toLocaleString("pt-BR", { minimumFractionDigits: 1, maximumFractionDigits: 1 })} GB`;

/**
 * The last reading, kept past the sidebar: hidden and shown again, the sidebar draws the bar at once instead of
 * growing it under the list a moment later, which pushed the rows up while they were being pulled at.
 */
const reading = signal<Memory | null>(null);

export function MemoryMeter() {
  useEffect(() => {
    const read = () =>
      void memoryNow()
        .then((m) => (reading.value = m))
        .catch(() => (reading.value = null));
    read();
    const timer = setInterval(read, 3000);
    return () => clearInterval(timer);
  }, []);
  const memory = reading.value;
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
