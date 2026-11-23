import type {
  ChangeSetEvaluation,
  ChangeSetImpact,
  ChangeSetRecord,
  DocumentDiff,
} from '../api/StudioClient';

export type { ChangeSetEvaluation, ChangeSetImpact, ChangeSetRecord, DocumentDiff };

export type ChangeSetClient = {
  createChangeSet(baseRevision: number, title: string): Promise<{ id: string; version: number }>;
  addChangeSetOperations(
    changeSetId: string,
    expectedVersion: number,
    operations: Record<string, unknown>[],
    source: { kind: 'human' | 'agent'; label?: string },
  ): Promise<{ version: number }>;
  removeChangeSetOperations(
    changeSetId: string,
    expectedVersion: number,
    entryIds: string[],
  ): Promise<{ version: number }>;
  evaluateChangeSet(changeSetId: string, expectedVersion: number): Promise<ChangeSetEvaluation>;
  changeSet?(changeSetId: string): Promise<ChangeSetRecord>;
  changeSetCandidate?(changeSetId: string): Promise<ChangeSetEvaluation>;
  commitChangeSet(
    changeSetId: string,
    expectedVersion: number,
    approvedDigest: string,
    idempotencyKey: string,
  ): Promise<{ revision: number }>;
  discardChangeSet?(changeSetId: string): Promise<ChangeSetRecord>;
  candidatePreview?(candidateDigest: string): Promise<{ url: string }>;
  /** 代价预测；旧客户端可以省略，面板会退化成只显示 diff。 */
  changeSetImpact?(changeSetId: string, expectedVersion: number): Promise<ChangeSetImpact>;
};
