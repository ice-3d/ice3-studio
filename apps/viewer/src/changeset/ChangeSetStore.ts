import type {
  ChangeSetClient,
  ChangeSetEvaluation,
  ChangeSetImpact,
  ChangeSetRecord,
} from './types';
import { canMutateChangeSet } from './reviewModel';
import { ExternalStore } from '../shared/ExternalStore';

export type ChangeSetViewStatus =
  | 'idle'
  | 'staging'
  | 'evaluating'
  | 'draft'
  | 'ready'
  | 'blocked'
  | 'stale'
  | 'committed'
  | 'discarded'
  | 'error';

export type ChangeSetSnapshot = {
  active: ChangeSetEvaluation | null;
  record: ChangeSetRecord | null;
  reviewedDigest: string | null;
  canApprove: boolean;
  status: ChangeSetViewStatus;
  candidateUrl: string | null;
  /** 代价预测；拉不到时为 null，面板退化为只显示 diff。 */
  impact: ChangeSetImpact | null;
  error: string | null;
};

export class ChangeSetStore extends ExternalStore<ChangeSetSnapshot> {
  constructor(private readonly client: ChangeSetClient) {
    super({
      active: null,
      record: null,
      reviewedDigest: null,
      canApprove: false,
      status: 'idle',
      candidateUrl: null,
      impact: null,
      error: null,
    });
  }

  acceptEvaluation(evaluation: ChangeSetEvaluation): void {
    const status = normalizeStatus(evaluation.status);
    const canApprove = status === 'ready'
      && evaluation.candidateDigest !== null
      && evaluation.candidateDigest === this.snapshot.reviewedDigest;
    this.update({
      ...this.snapshot,
      active: evaluation,
      canApprove,
      status,
      candidateUrl: status === 'ready' ? this.snapshot.candidateUrl : null,
      error: null,
    });
  }

  updateRecord(record: ChangeSetRecord): void {
    this.update({
      ...this.snapshot,
      record,
      status: normalizeStatus(record.status),
    });
  }

  updateCandidatePreview(url: string | null): void {
    this.update({ ...this.snapshot, candidateUrl: url });
  }

  markReviewed(digest: string): void {
    const canApprove = this.snapshot.active?.candidateDigest === digest;
    this.update({ ...this.snapshot, reviewedDigest: digest, canApprove });
  }

  async evaluate(): Promise<void> {
    const active = this.snapshot.active;
    if (!active) return;
    this.update({
      ...this.snapshot,
      status: 'evaluating',
      canApprove: false,
      candidateUrl: null,
      impact: null,
    });
    const evaluation = await this.client.evaluateChangeSet(active.changeSetId, active.version);
    this.acceptEvaluation(evaluation);
    await Promise.all([
      this.loadImpact(evaluation.changeSetId, evaluation.version),
      this.refreshRecord(evaluation.changeSetId),
    ]);
  }

  async addOperations(
    baseRevision: number,
    operations: Record<string, unknown>[],
  ): Promise<void> {
    if (operations.length === 0) return;
    this.update({ ...this.snapshot, status: 'staging' });
    const current = this.snapshot.active;
    const changeSet = current
      ? { changeSetId: current.changeSetId, version: current.version }
      : await this.client.createChangeSet(baseRevision, 'Workbench form edits')
        .then((created) => ({ changeSetId: created.id, version: created.version }));
    const updated = await this.client.addChangeSetOperations(
      changeSet.changeSetId,
      changeSet.version,
      operations,
      { kind: 'human', label: 'Inspector' },
    );
    await this.evaluateAt(changeSet.changeSetId, updated.version);
  }

  async removeEntry(entryId: string): Promise<void> {
    const active = this.snapshot.active;
    const record = this.snapshot.record;
    if (!active || !record || !canMutateChangeSet(record.status)) {
      throw new Error('ChangeSet is not mutable');
    }
    this.update({
      ...this.snapshot,
      status: 'evaluating',
      candidateUrl: null,
      canApprove: false,
      error: null,
    });
    try {
      const updated = await this.client.removeChangeSetOperations(
        active.changeSetId,
        active.version,
        [entryId],
      );
      await this.evaluateAt(active.changeSetId, updated.version);
    } catch (error) {
      this.update({
        ...this.snapshot,
        status: 'error',
        error: error instanceof Error ? error.message : 'ChangeSet update failed',
      });
    }
  }

  async commit(): Promise<number> {
    const active = this.snapshot.active;
    if (!active?.candidateDigest || !this.snapshot.canApprove) {
      throw new Error('Candidate must be reviewed before approval');
    }
    const result = await this.client.commitChangeSet(
      active.changeSetId,
      active.version,
      active.candidateDigest,
      `approval-${active.changeSetId}-${active.candidateDigest}`,
    );
    await this.refreshRecord(active.changeSetId);
    this.update({
      ...this.snapshot,
      active: null,
      status: 'committed',
      reviewedDigest: null,
      canApprove: false,
      candidateUrl: null,
      impact: null,
      error: null,
      ...(this.snapshot.record
        ? {
            record: {
              ...this.snapshot.record,
              status: 'committed',
              committedRevision: result.revision,
            },
          }
        : {}),
    });
    return result.revision;
  }

  /// Cost prediction is advisory: a failure must not block review.
  private async loadImpact(changeSetId: string, version: number): Promise<void> {
    if (!this.client.changeSetImpact) return;
    try {
      const impact = await this.client.changeSetImpact(changeSetId, version);
      this.update({ ...this.snapshot, impact });
    } catch {
      this.update({ ...this.snapshot, impact: null });
    }
  }

  private async evaluateAt(changeSetId: string, version: number): Promise<void> {
    this.update({
      ...this.snapshot,
      status: 'evaluating',
      canApprove: false,
      candidateUrl: null,
      impact: null,
      error: null,
    });
    const evaluation = await this.client.evaluateChangeSet(changeSetId, version);
    this.update({
      ...this.snapshot,
      reviewedDigest: null,
      active: evaluation,
    });
    await Promise.all([
      this.loadImpact(changeSetId, version),
      this.refreshRecord(changeSetId),
    ]);
    this.acceptEvaluation(evaluation);
    if (evaluation.candidateDigest && this.client.candidatePreview) {
      try {
        const preview = await this.client.candidatePreview(evaluation.candidateDigest);
        this.update({ ...this.snapshot, candidateUrl: preview.url });
      } catch (error) {
        this.update({
          ...this.snapshot,
          candidateUrl: null,
          error: error instanceof Error ? error.message : 'Candidate preview failed',
        });
      }
    }
  }

  private async refreshRecord(changeSetId: string): Promise<void> {
    if (!this.client.changeSet) return;
    try {
      this.update({
        ...this.snapshot,
        record: await this.client.changeSet(changeSetId),
      });
    } catch (error) {
      this.update({
        ...this.snapshot,
        error: error instanceof Error ? error.message : 'ChangeSet refresh failed',
      });
    }
  }
}

function normalizeStatus(status: string): ChangeSetViewStatus {
  const normalized = status.toLowerCase();
  if (
    normalized === 'draft'
    || normalized === 'evaluating'
    || normalized === 'ready'
    || normalized === 'blocked'
    || normalized === 'stale'
    || normalized === 'committed'
    || normalized === 'discarded'
  ) {
    return normalized;
  }
  return 'error';
}
