import type { ComputeStatus } from '../api/StudioClient';

export function formatComputeStatus(status: ComputeStatus | null): {
  title: string;
  detail: string;
  state: 'starting' | 'ready' | 'fallback' | 'unavailable';
  fallbackReason: string | null;
} {
  if (!status) {
    return {
      title: 'Native compute · starting',
      detail: 'Waiting for capabilities',
      state: 'starting',
      fallbackReason: null,
    };
  }
  const backend = status.selectedBackend === 'gpu' ? 'GPU' : 'CPU';
  const kernel = status.lastKernelMs > 0
    ? `kernel ${formatDecimal(status.lastKernelMs)} ms`
    : 'no kernel sample';
  const readback = status.lastReadbackBytes > 0
    ? `${formatDecimal(status.lastReadbackBytes / 1024)} KiB readback`
    : 'no readback yet';
  return {
    title: `Native compute · ${backend}`,
    detail: `${status.tier} · ${kernel} · ${readback}`,
    state: status.health === 'ready'
      ? 'ready'
      : status.fallbackReason
        ? 'fallback'
        : 'unavailable',
    fallbackReason: status.fallbackReason,
  };
}

function formatDecimal(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

function formatBytes(value: number): string {
  if (value >= 1024 ** 3) return `${formatDecimal(value / 1024 ** 3)} GiB`;
  if (value >= 1024 ** 2) return `${formatDecimal(value / 1024 ** 2)} MiB`;
  return `${formatDecimal(value / 1024)} KiB`;
}

export function ComputePanel({ status }: { status: ComputeStatus | null }) {
  const view = formatComputeStatus(status);
  return (
    <section className="compute-panel" aria-label="Native compute status">
      <div>
        <span className="toolchain-panel__eyebrow">Geometry engine</span>
        <strong>{view.title}</strong>
      </div>
      <p>{view.detail}</p>
      {status ? (
        <p className="compute-panel__metrics">
          {formatBytes(status.memoryBudgetBytes)} budget · {formatBytes(status.lastUploadBytes)} upload ·{' '}
          {status.lastCacheHits} cache hits · queue {status.queueDepth}/{status.maxConcurrentJobs}
        </p>
      ) : null}
      {view.fallbackReason ? <p className="toolchain-panel__reason">{view.fallbackReason}</p> : null}
    </section>
  );
}
