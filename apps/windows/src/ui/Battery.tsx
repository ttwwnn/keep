// The battery: a cell with its percentage written inside it, and its charge
// in a colour — green while there is plenty or the computer is charging,
// amber from 20% down, red from 10%.
//
// It stands in the sidebar's top row whenever the screen has no taskbar of
// its own to show the system's: a window in full screen. On the Mac it stands
// there while the menu bar, which carries the system's, is out of sight. A
// computer without a battery has nothing to show.

import { signal } from "@preact/signals";

interface Reading {
  percent: number;
  charging: boolean;
  onPower: boolean;
}

interface BatteryManager extends EventTarget {
  level: number;
  charging: boolean;
  chargingTime: number;
}

const reading = signal<Reading | null>(null);
const fullscreen = signal(false);
let started = false;

function measureScreen() {
  fullscreen.value = window.innerHeight >= screen.height - 1 && window.innerWidth >= screen.width - 1;
}

/** Listen to the system once; the signal says what it last said. */
function start() {
  if (started) return;
  started = true;
  measureScreen();
  window.addEventListener("resize", measureScreen);
  const ask = (navigator as unknown as { getBattery?: () => Promise<BatteryManager> }).getBattery;
  if (!ask) return;
  void ask.call(navigator).then((battery) => {
    const read = () => {
      const percent = Math.min(100, Math.max(0, Math.round(battery.level * 100)));
      reading.value = {
        percent,
        charging: battery.charging && percent < 100,
        onPower: battery.charging,
      };
    };
    read();
    for (const event of ["levelchange", "chargingchange"]) battery.addEventListener(event, read);
    // A desktop reports a battery that is always full and always charging.
    if (battery.charging && battery.level === 1 && battery.chargingTime === 0) {
      // Indistinguishable from a laptop on its charger; shown all the same.
    }
  });
}

const sentence = (r: Reading) =>
  `Bateria em ${r.percent}%, ${r.charging ? "carregando" : r.onPower ? "carregada, na tomada" : "na bateria"}`;

export function Battery() {
  start();
  const r = reading.value;
  if (!r || !fullscreen.value) return null;
  const tone = r.charging || r.onPower || r.percent > 20 ? "live" : r.percent > 10 ? "attention" : "critical";
  return (
    <div class={`battery ${tone}`} title={sentence(r)} aria-label={sentence(r)} role="img" style={{ "--p": r.percent / 100 }}>
      {r.charging && (
        <svg class="battery-bolt" viewBox="0 0 8 12" aria-hidden="true">
          <path d="M5 0L0 7h3.2L2.6 12 8 4.6H4.8z" fill="currentColor" />
        </svg>
      )}
      <div class="battery-body">
        <div class="battery-charge" />
        <span class="battery-size">100%</span>
        <span class="battery-text">{r.percent}%</span>
        <span class="battery-text over">{r.percent}%</span>
      </div>
      <span class="battery-tip" />
    </div>
  );
}
