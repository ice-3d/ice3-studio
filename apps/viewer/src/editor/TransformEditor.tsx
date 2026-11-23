import { useEffect, useState } from 'react';
import type { StudioMaterial, StudioVec3 } from '../api/StudioClient';
import {
  eulerDegreesToQuaternion,
  quaternionToEulerDegrees,
} from './transformMath';
import type {
  EditorDraft,
  EditorValues,
  GizmoMode,
  PrimitiveValue,
  TransformSpace,
} from './types';

type Props = {
  values: EditorValues;
  draft: EditorDraft;
  materials: readonly StudioMaterial[];
  mode: GizmoMode;
  space: TransformSpace;
  onValues(values: EditorValues): void;
  onPreviewStart(): void;
  onMode(mode: GizmoMode): void;
  onSpace(space: TransformSpace): void;
  onUndo(): void;
  onRedo(): void;
  onStage(): void;
  onDiscard(): void;
};

export function TransformEditor(props: Props) {
  const euler = quaternionToEulerDegrees(props.values.transform.rotation);
  const updateNumber = (
    section: 'translation' | 'rotation' | 'scale',
    axis: keyof StudioVec3,
    value: number,
  ): void => {
    const next = structuredClone(props.values);
    if (section === 'rotation') {
      next.transform.rotation = eulerDegreesToQuaternion({ ...euler, [axis]: value });
    } else {
      next.transform[section] = { ...next.transform[section], [axis]: value };
    }
    props.onValues(next);
  };
  return (
    <section className="editor-section" aria-label="Transform editor">
      <header className="editor-section__heading">
        <span className="panel-kicker">Transform</span>
        <div className="segmented-control" aria-label="Transform mode">
          {(['move', 'rotate', 'scale'] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              aria-label={`${capitalize(mode)} mode`}
              aria-pressed={props.mode === mode}
              onClick={() => props.onMode(mode)}
            >
              {capitalize(mode)}
            </button>
          ))}
        </div>
      </header>
      <TransformRow
        label="Position"
        prefix="Position"
        values={props.values.transform.translation}
        onChange={(axis, value) => updateNumber('translation', axis, value)}
      />
      <TransformRow
        label="Rotation"
        prefix="Rotation"
        values={euler}
        unit="deg"
        onChange={(axis, value) => updateNumber('rotation', axis, value)}
      />
      <TransformRow
        label="Scale"
        prefix="Scale"
        values={props.values.transform.scale}
        positiveOnly
        onChange={(axis, value) => updateNumber('scale', axis, value)}
      />
      <label className="field">
        <span>Transform space</span>
        <select
          value={props.space}
          onChange={(event) => props.onSpace(event.target.value as TransformSpace)}
        >
          <option value="local">Local</option>
          <option value="world">World</option>
        </select>
      </label>
      {props.values.primitive && (
        <fieldset className="parameter-group">
          <legend>
            <span>Geometry</span>
            <small>{props.values.primitive.kind}</small>
          </legend>
          {primitiveFields(props.values.primitive).map(([key, value]) => (
            <label className="field" key={key}>
              <span>{formatLabel(key)}</span>
              <NumericInput
                label={`Geometry ${formatLabel(key)}`}
                value={value}
                positiveOnly
                onCommit={(nextValue) => {
                  const next = structuredClone(props.values);
                  if (next.primitive) {
                    (next.primitive as Record<string, unknown>)[key] = nextValue;
                  }
                  props.onValues(next);
                }}
              />
            </label>
          ))}
        </fieldset>
      )}
      <label className="field">
        <span>Material</span>
        <select
          value={props.values.materialId ?? ''}
          onChange={(event) => {
            const next = structuredClone(props.values);
            next.materialId = event.target.value || null;
            props.onValues(next);
          }}
        >
          <option value="">No material</option>
          {props.materials.map((material) => (
            <option value={material.id} key={material.id}>
              {material.name}
            </option>
          ))}
        </select>
      </label>
      <DraftActions {...props} />
      {props.draft.error && (
        <p className="form-error" role="alert">{props.draft.error}</p>
      )}
    </section>
  );
}

function TransformRow({
  label,
  prefix,
  values,
  unit,
  positiveOnly = false,
  onChange,
}: {
  label: string;
  prefix: string;
  values: StudioVec3;
  unit?: string;
  positiveOnly?: boolean;
  onChange(axis: keyof StudioVec3, value: number): void;
}) {
  return (
    <div className="transform-row" role="group" aria-label={`${label} values`}>
      <span className="transform-row__label">
        {label}
        {unit && <small>{unit}</small>}
      </span>
      {(['x', 'y', 'z'] as const).map((axis) => (
        <NumericInput
          key={axis}
          label={`${prefix} ${axis.toUpperCase()}`}
          value={values[axis]}
          positiveOnly={positiveOnly}
          onCommit={(value) => onChange(axis, value)}
        />
      ))}
    </div>
  );
}

function NumericInput({
  label,
  value,
  positiveOnly,
  onCommit,
}: {
  label: string;
  value: number;
  positiveOnly: boolean;
  onCommit(value: number): void;
}) {
  const formatted = formatNumber(value);
  const [text, setText] = useState(formatted);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setText(formatted);
  }, [formatted]);

  const commit = (): void => {
    const next = Number(text);
    if (!Number.isFinite(next)) {
      setError('Enter a finite number');
      return;
    }
    if (positiveOnly && next <= 0) {
      setError('Value must be positive');
      return;
    }
    setError(null);
    if (next !== value) onCommit(next);
  };

  return (
    <>
      <input
        aria-label={label}
        aria-invalid={error ? 'true' : undefined}
        inputMode="decimal"
        value={text}
        onChange={(event) => {
          setText(event.target.value);
          setError(null);
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault();
            commit();
            event.currentTarget.blur();
          }
          if (event.key === 'Escape') {
            setText(formatted);
            setError(null);
            event.currentTarget.blur();
          }
        }}
      />
      {error && <span className="field-error" role="alert">{error}</span>}
    </>
  );
}

function DraftActions(props: Props) {
  const canStage = props.draft.status !== 'staged' && props.draft.status !== 'stale';
  return (
    <div className="editor-actions">
      <div className={`draft-state draft-state--${props.draft.status}`}>
        <span className="status-dot" aria-hidden="true" />
        {props.draft.status}
      </div>
      <div className="editor-actions__buttons">
        <button
          type="button"
          className="button button--quiet"
          aria-label="Undo draft"
          disabled={props.draft.history.length === 0}
          onClick={props.onUndo}
        >
          Undo
        </button>
        <button
          type="button"
          className="button button--quiet"
          aria-label="Redo draft"
          disabled={props.draft.future.length === 0}
          onClick={props.onRedo}
        >
          Redo
        </button>
        <button
          type="button"
          className="button button--quiet"
          aria-label="Discard draft"
          onClick={props.onDiscard}
        >
          Discard
        </button>
        <button
          type="button"
          className="button button--primary"
          disabled={!canStage}
          onClick={props.onStage}
        >
          Stage draft
        </button>
      </div>
    </div>
  );
}

function primitiveFields(value: PrimitiveValue): Array<[string, number]> {
  if (value.kind === 'box') {
    return [['width', value.width], ['height', value.height], ['depth', value.depth]];
  }
  if (value.kind === 'plane') {
    return [['width', value.width], ['depth', value.depth]];
  }
  if (value.kind === 'cylinder') {
    return [
      ['radius', value.radius],
      ['height', value.height],
      ['segments', value.segments],
    ];
  }
  return [
    ['radius', value.radius],
    ['segments', value.segments],
    ['rings', value.rings],
  ];
}

function formatNumber(value: number): string {
  return String(Number(value.toFixed(3)));
}

function formatLabel(value: string): string {
  return value
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/^./, (character) => character.toUpperCase());
}

function capitalize(value: string): string {
  return `${value.charAt(0).toUpperCase()}${value.slice(1)}`;
}
