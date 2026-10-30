import { createRoot } from 'react-dom/client';
import { Workbench } from './app/Workbench';

const root = document.getElementById('app');
if (!root) throw new Error('Workbench root is missing');
createRoot(root).render(<Workbench />);
