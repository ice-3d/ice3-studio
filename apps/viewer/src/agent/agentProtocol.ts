import type { AgentEvent, AgentIntent, AgentToolPhase } from './transport';

export type AgentClientMessage = {
  type: 'intent.submit';
  protocolVersion: 1;
  requestId: string;
  intent: AgentIntent;
} | {
  type: 'run.cancel';
  protocolVersion: 1;
  runId: string;
} | {
  type: 'ping';
  protocolVersion: 1;
};

export type AgentServerMessage = AgentEvent;

export function serializeAgentClientMessage(message: AgentClientMessage): string {
  return JSON.stringify(message);
}

export function parseAgentServerMessage(value: unknown): AgentServerMessage | null {
  if (!isRecord(value) || value.protocolVersion !== 1 || typeof value.type !== 'string') {
    return null;
  }
  switch (value.type) {
    case 'gateway.unavailable':
      return typeof value.message === 'string'
        ? { type: value.type, protocolVersion: 1, message: value.message }
        : null;
    case 'run.started':
      return stringField(value, 'requestId') && stringField(value, 'runId')
        ? {
            type: value.type,
            protocolVersion: 1,
            requestId: value.requestId,
            runId: value.runId,
          }
        : null;
    case 'message.delta':
      return requiredRunId(value) && stringField(value, 'text')
        ? { type: value.type, protocolVersion: 1, runId: value.runId, text: value.text }
        : null;
    case 'plan.updated':
      return requiredRunId(value) && stringField(value, 'summary')
        ? {
            type: value.type,
            protocolVersion: 1,
            runId: value.runId,
            summary: value.summary,
          }
        : null;
    case 'tool.updated':
      return requiredRunId(value)
        && stringField(value, 'toolName')
        && isToolPhase(value.phase)
        && typeof value.summary === 'string'
        ? {
            type: value.type,
            protocolVersion: 1,
            runId: value.runId,
            toolName: value.toolName,
            phase: value.phase,
            summary: value.summary,
          }
        : null;
    case 'changeset.linked':
      return requiredRunId(value)
        && stringField(value, 'changeSetId')
        && stringArray(value.entryIds)
        ? {
            type: value.type,
            protocolVersion: 1,
            runId: value.runId,
            changeSetId: value.changeSetId,
            entryIds: value.entryIds,
          }
        : null;
    case 'run.finished':
      return requiredRunId(value)
        ? { type: value.type, protocolVersion: 1, runId: value.runId }
        : null;
    case 'run.failed':
      return requiredRunId(value)
        && stringField(value, 'diagnosticCode')
        && stringField(value, 'message')
        ? {
            type: value.type,
            protocolVersion: 1,
            runId: value.runId,
            diagnosticCode: value.diagnosticCode,
            message: value.message,
          }
        : null;
    case 'pong':
      return { type: value.type, protocolVersion: 1 };
    default:
      return null;
  }
}

function requiredRunId(
  value: Record<string, unknown>,
): value is Record<string, unknown> & { runId: string } {
  return stringField(value, 'runId');
}

function stringField<K extends string>(
  value: Record<string, unknown>,
  key: K,
): value is Record<string, unknown> & Record<K, string> {
  return typeof value[key] === 'string' && value[key].length > 0;
}

function stringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === 'string');
}

function isToolPhase(value: unknown): value is AgentToolPhase {
  return value === 'started' || value === 'finished' || value === 'failed';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
