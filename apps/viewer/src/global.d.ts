declare const __STUDIO_TOKEN__: string;

interface Window {
  __studioTest: {
    publishRevision(revision: number): void;
  };
}
