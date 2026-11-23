import { useEffect, useState } from 'react';
import type {
  ComponentDefinition,
  ComponentInstance,
  StudioNode,
} from '../api/StudioClient';
import { compileParameterChanges, formatParameter } from './instanceParameters';
import type { EditorValues } from '../editor/types';

type Props = {
  node: StudioNode | undefined;
  definition?: ComponentDefinition;
  values?: EditorValues;
  onValues(values: EditorValues): void;
};

export function FormRenderer({ node, definition, values, onValues }: Props) {
  const [name, setName] = useState('');
  const [visible, setVisible] = useState(true);
  const [parameters, setParameters] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setName(values?.name ?? node?.name ?? '');
    setVisible(values?.visible ?? node?.visible ?? true);
    setParameters(values?.componentParameters ?? {});
    setError(null);
  }, [node?.id, values]);

  if (!node || !values) {
    return (
      <div className="empty-state inspector-empty">
        <span className="empty-state__icon" aria-hidden="true">+</span>
        <strong>No object selected</strong>
        <span>Choose an entity in the viewport or object browser.</span>
      </div>
    );
  }

  const commit = (draft: {
    name?: string;
    visible?: boolean;
    componentParameters?: Record<string, string>;
  }): void => {
    const nextParameters = draft.componentParameters ?? parameters;
    if (definition && node.content?.type === 'component_instance') {
      try {
        compileParameterChanges(
          definition,
          node.id,
          node.content as unknown as ComponentInstance,
          nextParameters,
        );
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
        return;
      }
    }
    setError(null);
    onValues({
      ...values,
      name: draft.name ?? name,
      visible: draft.visible ?? visible,
      componentParameters: nextParameters,
    });
  };

  return (
    <form
      className="inspector-form"
      aria-label={`Inspect ${node.name}`}
      onSubmit={(event) => event.preventDefault()}
    >
      <div className="inspector-form__heading">
        <div>
          <span className="panel-kicker">Selection</span>
          <h2>{node.name}</h2>
        </div>
        <label className="visibility-toggle">
          <input
            type="checkbox"
            checked={visible}
            onChange={(event) => {
              setVisible(event.target.checked);
              commit({ visible: event.target.checked });
            }}
          />
          <span aria-hidden="true" />
          Visible
        </label>
      </div>
      <div className="field-stack">
        <label className="field">
          <span>Name</span>
          <input
            value={name}
            onChange={(event) => {
              setName(event.target.value);
              commit({ name: event.target.value });
            }}
            onBlur={() => commit({ name })}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                commit({ name });
              }
            }}
          />
        </label>
      </div>
      {definition && node.content?.type === 'component_instance' && (
        <fieldset className="parameter-group">
          <legend>
            <span>Geometry parameters</span>
            <small>{definition.name}</small>
          </legend>
          {definition.parameters.map((parameter) => (
            <label className="field" key={parameter.name}>
              <span>
                {formatLabel(parameter.name)}
                {parameter.unit ? <small>{parameter.unit}</small> : null}
              </span>
              {parameter.valueType.kind === 'enumeration' ? (
                <select
                  aria-label={parameter.name}
                  value={parameters[parameter.name] ?? ''}
                  onChange={(event) => {
                    const next = {
                      ...parameters,
                      [parameter.name]: event.target.value,
                    };
                    setParameters(next);
                    commit({ componentParameters: next });
                  }}
                >
                  {(parameter.valueType.values as string[]).map((value) => (
                    <option value={value} key={value}>{value}</option>
                  ))}
                </select>
              ) : (
                <input
                  aria-label={parameter.name}
                  inputMode="decimal"
                  value={
                    parameters[parameter.name]
                    ?? formatParameter(parameter.default)
                  }
                  onChange={(event) => {
                    const next = {
                      ...parameters,
                      [parameter.name]: event.target.value,
                    };
                    setParameters(next);
                    commit({ componentParameters: next });
                  }}
                  onBlur={() => commit({ componentParameters: parameters })}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault();
                      commit({ componentParameters: parameters });
                    }
                  }}
                />
              )}
            </label>
          ))}
        </fieldset>
      )}
      {error && <p className="form-error" role="alert">{error}</p>}
    </form>
  );
}

function formatLabel(value: string): string {
  return value
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/^./, (character) => character.toUpperCase());
}
