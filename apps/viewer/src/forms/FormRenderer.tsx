import { useEffect, useState } from 'react';
import type { StudioNode } from '../api/StudioClient';
import { compileNodeChanges, type Operation } from './operationCompiler';

type Props = {
  node: StudioNode | undefined;
  onStage: (operations: Operation[]) => void;
};

export function FormRenderer({ node, onStage }: Props) {
  const [name, setName] = useState('');
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    setName(node?.name ?? '');
    setVisible(node?.visible ?? true);
  }, [node?.id, node?.name, node?.visible]);

  if (!node) return <p>Select an object to inspect.</p>;
  return (
    <form
      aria-label={`Inspect ${node.name}`}
      onSubmit={(event) => {
        event.preventDefault();
        onStage(compileNodeChanges(node, { name, visible }));
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
      <button type="submit">Stage changes</button>
    </form>
  );
}
