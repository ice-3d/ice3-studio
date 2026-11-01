import type { ToolchainStatus } from '../api/StudioClient';

export function formatToolchainStatus(status: ToolchainStatus | null): {
  label: string;
  version: string;
  reason: string | null;
} {
  if (!status) {
    return {
      label: 'Toolchain starting',
      version: 'Waiting for capabilities',
      reason: null,
    };
  }
  const label = status.state === 'ready'
    ? 'Toolchain ready'
    : status.state === 'busy'
      ? 'Toolchain busy'
      : status.state === 'incompatible'
        ? 'Toolchain incompatible'
        : status.state === 'failed'
          ? 'Toolchain failed'
          : 'Toolchain unavailable';
  return {
    label,
    version: status.contractVersion && status.packageVersion
      ? `contract ${status.contractVersion} · package ${status.packageVersion}`
      : 'Version unavailable',
    reason: status.reason,
  };
}

export function ToolchainPanel({ status }: { status: ToolchainStatus | null }) {
  const view = formatToolchainStatus(status);
  const state = status?.state ?? 'starting';
  return (
    <section className={`status-card toolchain-panel status-card--${state}`} aria-label="Toolchain status">
      <div className="status-card__heading">
        <span className="toolchain-panel__eyebrow">Asset pipeline</span>
        <strong>
          <span className="status-dot" aria-hidden="true" />
          {view.label}
        </strong>
      </div>
      <p className="status-card__detail">{view.version}</p>
      {status?.capabilities.length ? (
        <div className="capability-list">
          {status.capabilities.map((capability) => (
            <span key={capability}>{capability}</span>
          ))}
        </div>
      ) : null}
      {view.reason ? <p className="toolchain-panel__reason">{view.reason}</p> : null}
    </section>
  );
}
