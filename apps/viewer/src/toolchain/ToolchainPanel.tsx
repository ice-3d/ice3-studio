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
  return (
    <section className="toolchain-panel" aria-label="Toolchain status">
      <div>
        <span className="toolchain-panel__eyebrow">Asset pipeline</span>
        <strong>{view.label}</strong>
      </div>
      <p>{view.version}</p>
      {status?.capabilities.length ? (
        <p className="toolchain-panel__capabilities">
          {status.capabilities.join(' · ')}
        </p>
      ) : null}
      {view.reason ? <p className="toolchain-panel__reason">{view.reason}</p> : null}
    </section>
  );
}
