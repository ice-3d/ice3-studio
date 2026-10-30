import type { ChangeSetClient, ChangeSetEvaluation } from './types';
import { ExternalStore } from '../shared/ExternalStore';

export type ChangeSetSnapshot = {
  active: ChangeSetEvaluation | null;
  reviewedDigest: string | null;
  canApprove: boolean;
  status: 'idle' | 'staging' | 'evaluating' | 'ready' | 'blocked' | 'error';
  candidateUrl: string | null;
};

export class ChangeSetStore extends ExternalStore<ChangeSetSnapshot> {
  constructor(private readonly client: ChangeSetClient) {
    super({
      active: null,
      reviewedDigest: null,
      canApprove: false,
      status: 'idle',
      candidateUrl: null,
    });
  }

  acceptEvaluation(evaluation: ChangeSetEvaluation): void {
    const canApprove = evaluation.status === 'ready'
      && evaluation.candidateDigest !== null
      && evaluation.candidateDigest === this.snapshot.reviewedDigest;
    this.update({
      ...this.snapshot,
      active: evaluation,
      canApprove,
      status: evaluation.status === 'ready' ? 'ready' : 'blocked',
      candidateUrl: this.snapshot.candidateUrl,
    });
  }

  markReviewed(digest: string): void {
    const canApprove = this.snapshot.active?.candidateDigest === digest;
    this.update({ ...this.snapshot, reviewedDigest: digest, canApprove });
  }

  async evaluate(): Promise<void> {
    const active = this.snapshot.active;
    if (!active) return;
    this.update({ ...this.snapshot, status: 'evaluating', canApprove: false });
    const evaluation = await this.client.evaluateChangeSet(active.changeSetId, active.version);
    this.acceptEvaluation(evaluation);
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
    return result.revision;
  }

  private async evaluateAt(changeSetId: string, version: number): Promise<void> {
    this.update({ ...this.snapshot, status: 'evaluating', canApprove: false });
    const evaluation = await this.client.evaluateChangeSet(changeSetId, version);
    this.update({ ...this.snapshot, reviewedDigest: null, active: evaluation });
    this.acceptEvaluation(evaluation);
    if (evaluation.candidateDigest && this.client.candidatePreview) {
      const preview = await this.client.candidatePreview(evaluation.candidateDigest);
      this.update({ ...this.snapshot, candidateUrl: preview.url });
    }
  }
}
