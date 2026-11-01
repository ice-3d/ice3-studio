import { useEffect, useState } from 'react';
import type {
  ComponentDefinition,
  ComponentInstance,
  StudioNode,
} from '../api/StudioClient';
import { compileNodeChanges, type Operation } from './operationCompiler';
import {
  compileParameterChanges,
  formatParameter,
  type ParameterDraft,
} from './instanceParameters';

type Props = {
  node: StudioNode | undefined;
  definition?: ComponentDefinition;
  onStage: (operations: Operation[]) => void;
};

export function FormRenderer({ node, definition, onStage }: Props) {
  const [name, setName] = useState('');
  const [visible, setVisible] = useState(true);
  const [parameters, setParameters] = useState<ParameterDraft>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setName(node?.name ?? '');
    setVisible(node?.visible ?? true);
    const instance = node?.content?.type === 'component_instance'
      ? node.content as unknown as ComponentInstance
      : null;
    setParameters(Object.fromEntries(definition?.parameters.map((parameter) => [
      parameter.name,
      formatParameter(instance?.parameters[parameter.name] ?? parameter.default),
    ]) ?? []));
    setError(null);
  }, [definition, node?.id, node?.name, node?.visible]);

  if (!node) {
    return (
      <div className="empty-state inspector-empty">
        <span className="empty-state__icon" aria-hidden="true">+</span>
        <strong>No object selected</strong>
        <span>Choose an entity in the viewport or object browser.</span>
      </div>
    );
  }
  return (
    <form
      className="inspector-form"
      aria-label={`Inspect ${node.name}`}
      onSubmit={(event) => {
        event.preventDefault();
        try {
          const operations = compileNodeChanges(node, { name, visible });
          if (definition && node.content?.type === 'component_instance') {
            operations.push(...compileParameterChanges(
              definition,
              node.id,
              node.content as unknown as ComponentInstance,
              parameters,
            ));
          }
          setError(null);
          onStage(operations);
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      }}
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
            onChange={(event) => setVisible(event.target.checked)}
          />
          <span aria-hidden="true" />
          Visible
        </label>
      </div>
      <div className="field-stack">
        <label className="field">
          <span>Name</span>
          <input value={name} onChange={(event) => setName(event.target.value)} />
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
                  onChange={(event) => setParameters((current) => ({
                    ...current,
                    [parameter.name]: event.target.value,
                  }))}
                >
                  {(parameter.valueType.values as string[]).map((value) => (
                    <option value={value} key={value}>{value}</option>
                  ))}
                </select>
              ) : (
                <input
                  aria-label={parameter.name}
                  type="text"
                  inputMode="decimal"
                  value={parameters[parameter.name] ?? ''}
                  onChange={(event) => setParameters((current) => ({
                    ...current,
                    [parameter.name]: event.target.value,
                  }))}
                />
              )}
            </label>
          ))}
        </fieldset>
      )}
      {error && <p className="form-error" role="alert">{error}</p>}
      <div className="inspector-form__actions">
        <button type="submit" className="button button--secondary">Stage changes</button>
      </div>
    </form>
  );
}

function formatLabel(value: string): string {
  return value
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/^./, (character) => character.toUpperCase());
}
