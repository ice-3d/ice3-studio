import { useEffect, useState } from 'react';
import type {
  ComponentDefinition,
  ComponentInstance,
  StudioNode,
} from '../api/StudioClient';
import { compileNodeChanges, type Operation } from './operationCompiler';
import { compileParameterChanges, type ParameterDraft } from './instanceParameters';

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

  if (!node) return <p>Select an object to inspect.</p>;
  return (
    <form
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
      <label>Name <input value={name} onChange={(event) => setName(event.target.value)} /></label>
      <label>
        <input
          type="checkbox"
          checked={visible}
          onChange={(event) => setVisible(event.target.checked)}
        />
        Visible
      </label>
      {definition && node.content?.type === 'component_instance' && (
        <fieldset>
          <legend>{definition.name} parameters</legend>
          {definition.parameters.map((parameter) => (
            <label key={parameter.name}>
              {parameter.name}{parameter.unit ? ` (${parameter.unit})` : ''}
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
      {error && <p role="alert">{error}</p>}
      <button type="submit">Stage changes</button>
    </form>
  );
}

function formatParameter(value: ComponentInstance['parameters'][string]): string {
  if (value.kind === 'number') return String(value.value);
  if (value.kind === 'vector3') return [value.value.x, value.value.y, value.value.z].join(', ');
  if (value.kind === 'color') return value.value.join(', ');
  return value.value;
}
