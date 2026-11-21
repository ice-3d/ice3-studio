import { useEffect } from 'react';
import type {
  StudioDocument,
  StudioNode,
  StudioRouteInspectionPort,
} from '../api/StudioClient';
import {
  useRouteInspection,
  useRouteInspectionStore,
} from './RouteInspectionProvider';
import { routeLineIdForNode } from './routeInspectionModel';

type Props = {
  node: StudioNode | undefined;
  document: StudioDocument | null;
  revision: number;
  onSelect(nodeId: string): void;
};

export function RouteInspector({
  node,
  document,
  revision,
  onSelect,
}: Props) {
  const store = useRouteInspectionStore();
  const state = useRouteInspection();
  const lineId = routeLineIdForNode(node, document);

  useEffect(() => {
    if (!lineId || !document) {
      store.clear();
      return;
    }
    void store.load(lineId, revision);
  }, [document, lineId, revision, store]);

  if (!lineId) return null;
  return (
    <section aria-label="Route inspection" className="route-inspector">
      <div className="route-inspector__heading">
        <div>
          <span className="panel-kicker">Route</span>
          <h3>{lineId}</h3>
        </div>
        <span className="route-inspector__revision">Revision {revision}</span>
      </div>
      {state.status === 'loading' && <p role="status">Loading route facts…</p>}
      {state.status === 'error' && (
        <div className="route-inspector__error" role="alert">
          <span>{state.error}</span>
          <button
            type="button"
            className="button button--secondary"
            onClick={() => void store.load(lineId, revision)}
          >
            Retry
          </button>
        </div>
      )}
      {state.status === 'ready' && state.inspection && (
        <>
          {(state.inspection.truncated.segments
            || state.inspection.truncated.points
            || state.inspection.truncated.attachments
            || state.inspection.truncated.supports) && (
            <p className="route-inspector__warning">
              Partial route: response budget reached.
            </p>
          )}
          <section aria-labelledby="route-segments-heading">
            <h4 id="route-segments-heading">
              Segments · {state.inspection.segments.length}
            </h4>
            {state.inspection.segments.map((segment) => (
              <article className="route-inspector__segment" key={segment.nodeId}>
                <button
                  type="button"
                  className="route-inspector__link"
                  onClick={() => onSelect(segment.nodeId)}
                >
                  {segment.name}
                </button>
                <span>{segment.medium} · {segment.diameter.toFixed(3)} m</span>
                <span>{segment.routePolicy.kind} · {segment.points.length} points</span>
                <PortFact label="From" port={segment.from} onSelect={onSelect} />
                <PortFact label="To" port={segment.to} onSelect={onSelect} />
              </article>
            ))}
          </section>
          <RelatedFacts
            title="Attachments"
            items={state.inspection.attachments}
            onSelect={onSelect}
          />
          <RelatedFacts
            title="Supports"
            items={state.inspection.supports}
            onSelect={onSelect}
          />
        </>
      )}
    </section>
  );
}

function PortFact({
  label,
  port,
  onSelect,
}: {
  label: string;
  port: StudioRouteInspectionPort;
  onSelect(nodeId: string): void;
}) {
  return (
    <div className="route-inspector__fact">
      <span>{label}</span>
      <button
        type="button"
        className="route-inspector__link"
        onClick={() => onSelect(port.nodeId)}
      >
        {port.port}
      </button>
      <span>{port.diameter.toFixed(3)} m</span>
      <code>
        {port.position.x.toFixed(2)}, {port.position.y.toFixed(2)}, {port.position.z.toFixed(2)}
      </code>
      <code>
        {port.direction.x.toFixed(2)}, {port.direction.y.toFixed(2)}, {port.direction.z.toFixed(2)}
      </code>
    </div>
  );
}

function RelatedFacts({
  title,
  items,
  onSelect,
}: {
  title: string;
  items: Array<{
    nodeId: string;
    name: string;
    fittingKind?: string;
    hostName?: string | null;
    supportGroup?: string | null;
  }>;
  onSelect(nodeId: string): void;
}) {
  return (
    <section>
      <h4>{title} · {items.length}</h4>
      {items.map((item) => (
        <button
          type="button"
          className="route-inspector__related"
          key={item.nodeId}
          onClick={() => onSelect(item.nodeId)}
        >
          <span>{item.name}</span>
          <small>
            {[item.fittingKind, item.hostName, item.supportGroup, item.nodeId]
              .filter(Boolean)
              .join(' · ')}
          </small>
        </button>
      ))}
    </section>
  );
}
