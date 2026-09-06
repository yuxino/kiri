export interface CountdownTick { value: number; remaining: number }
export interface CountdownClock { start(): void; stop(): void }
export function createCountdownClock(options: {
  now(): number;
  requestFrame(callback: () => void): number;
  cancelFrame(id: number): void;
  onTick(tick: CountdownTick): void;
  onComplete(): void;
}): CountdownClock;
