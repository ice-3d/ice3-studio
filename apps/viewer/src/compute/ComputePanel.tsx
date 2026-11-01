import type { ComputeStatus } from '../api/StudioClient';

export function formatComputeStatus(status: ComputeStatus | null): {
  title: string;
  detail: string;
  state: 'starting' | 'ready' | 'fallback' | 'unavailable';
  fallbackReason: string | null;
  metrics: string;
} {
  if (!status) {
    return {
      title: 'Native compute · starting',
      detail: 'Waiting for capabilities',
      state: 'starting',
      fallbackReason: null,
      metrics: '',
    };
  }
  const backend = status.selectedBackend === 'gpu' ? 'GPU' : 'CPU';
  const kernel = status.lastKernelMs > 0
    ? `kernel ${formatDecimal(status.lastKernelMs)} ms`
    : 'no kernel sample';
  const mode = status.lastEvaluationMode ?? 'cold';
  const resident = status.residentGeometry;
  const residentBytes = status.residentBytes ?? resident?.residentBytes ?? 0;
  const peakResidentBytes = status.peakResidentBytes
    ?? resident?.peakResidentBytes
    ?? 0;
  const topologyHits = status.topologyHits ?? resident?.topologyHits ?? 0;
  const residentHits = status.residentHits ?? resident?.residentHits ?? 0;
  const evictions = status.evictions ?? resident?.evictions ?? 0;
  const packetBytes = status.previewPacketBytes ?? 0;
  const browserUploadBytes = status.browserUploadBytes ?? 0;
  const snapshotBytes = status.snapshotReadbackBytes ?? 0;
  const phases = status.lastGeometryPhases;
  const kernelMs = phases?.gpuKernelMs ?? status.lastKernelMs;
  const lookupMs = status.handleLookupMs ?? phases?.lookupMs ?? 0;
  const waitMs = phases?.waitMs ?? 0;
  const scene = status.sceneSession;
  const sceneDetail = scene?.enabled
    ? scene.state === 'pending'
      ? `scene pending ${scene.publishedRevision} → ${scene.targetRevision}`
      : scene.state === 'failed' || scene.state === 'lost'
        ? `scene ${scene.state}`
        : scene.publishedRevision > 0
          ? `scene ready r${scene.publishedRevision}`
          : 'scene ready'
    : null;
  const metrics = [
    `${formatBytes(status.memoryBudgetBytes)} budget`,
    `${formatBytes(status.lastUploadBytes)} upload`,
    `${status.lastCacheHits} cache hits`,
    `queue ${status.queueDepth}/${status.maxConcurrentJobs}`,
    `resident ${formatBytes(residentBytes)}`,
    `peak ${formatBytes(peakResidentBytes)}`,
    `topology ${topologyHits}`,
    `resident hits ${residentHits}`,
    `evictions ${evictions}`,
    `packet ${formatBytes(packetBytes)}`,
    `browser upload ${formatBytes(browserUploadBytes)}`,
    `snapshot ${formatBytes(snapshotBytes)}`,
    `lookup ${formatMillis(lookupMs)} ms`,
    `kernel ${formatMillis(kernelMs)} ms`,
    `wait ${formatMillis(waitMs)} ms`,
    `${status.residentHotPathHashes ?? 0} hot-path hashes`,
    ...(scene?.enabled
      ? [
          `${scene.instanceCount} instances`,
          `dirty ${formatBytes(scene.dirtyRangeBytes)}`,
          `scene readback ${formatBytes(scene.readbackBytes)}`,
          `${scene.pendingPatches} pending patches`,
        ]
      : []),
    `incremental scene ${status.incrementalSceneCache === false ? 'off' : 'on'}`,
  ].join(' · ');
  return {
    title: `Native compute · ${backend}`,
    detail: `${mode} · ${status.tier} · ${kernel} · readback ${formatBytes(status.lastReadbackBytes)}${sceneDetail ? ` · ${sceneDetail}` : ''}`,
    state: status.health === 'ready'
      ? 'ready'
      : status.fallbackReason
        ? 'fallback'
        : 'unavailable',
    fallbackReason: status.fallbackReason ?? scene?.fallbackReason ?? null,
    metrics,
  };
}

function formatDecimal(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

function formatMillis(value: number): string {
  if (value > 0 && value < 1) {
    return String(Number(value.toFixed(3)));
  }
  return formatDecimal(value);
}

function formatBytes(value: number): string {
  if (value < 1024) return `${formatDecimal(value)} B`;
  if (value >= 1024 ** 3) return `${formatDecimal(value / 1024 ** 3)} GiB`;
  if (value >= 1024 ** 2) return `${formatDecimal(value / 1024 ** 2)} MiB`;
  return `${formatDecimal(value / 1024)} KiB`;
}

export function ComputePanel({ status }: { status: ComputeStatus | null }) {
  const view = formatComputeStatus(status);
  return (
    <section className={`status-card compute-panel status-card--${view.state}`} aria-label="Native compute status">
      <div className="status-card__heading">
        <span className="toolchain-panel__eyebrow">Geometry engine</span>
        <strong>
          <span className="status-dot" aria-hidden="true" />
          {view.title}
        </strong>
      </div>
      <p className="status-card__detail">{view.detail}</p>
      {status ? (
        <div className="compute-panel__metrics">
          {view.metrics.split(' · ').map((metric) => (
            <span key={metric}>{metric}</span>
          ))}
        </div>
      ) : null}
      {view.fallbackReason ? <p className="toolchain-panel__reason">{view.fallbackReason}</p> : null}
    </section>
  );
}
