// Adapter for any OpenAI-compatible chat endpoint.
//
//   OPENAI_API_KEY   required
//   OPENAI_BASE_URL  default https://api.openai.com/v1
//   EVAL_MODEL       default gpt-4o-mini
//   EVAL_MAX_TURNS   default 8
//
// It reads the goal, discovers tools from `system.agentManifest`, and calls them
// until it stops. Grading still happens in tasks.mjs — this file only decides
// what to call.

const BASE_URL = process.env.OPENAI_BASE_URL ?? 'https://api.openai.com/v1';
const MODEL = process.env.EVAL_MODEL ?? 'gpt-4o-mini';
const MAX_TURNS = Number(process.env.EVAL_MAX_TURNS ?? '8');

/// OpenAI function names reject dots, while Studio methods are dotted.
const toApiName = (name) => name.replaceAll('.', '_');
const fromApiName = (name) => name.replaceAll('_', '.');

function resolvePointer(schema, pointer) {
  let node = schema;
  for (const segment of pointer.split('/').filter(Boolean)) {
    const key = segment.replaceAll('~1', '/').replaceAll('~0', '~');
    node = node?.[key];
    if (node === undefined) return undefined;
  }
  return node;
}

function inlineSchema(tool, schema) {
  const reference = tool.inputSchemaRef ?? '';
  const pointer = reference.includes('#') ? reference.slice(reference.indexOf('#') + 1) : '';
  return resolvePointer(schema, pointer) ?? { type: 'object', additionalProperties: true };
}

async function loadTools(context) {
  const manifest = await context.client.call('system.agentManifest', {});
  const response = await fetch(`${context.baseUrl}${manifest.schemaUrl}`);
  if (!response.ok) throw new Error(`schema fetch failed: ${response.status}`);
  const schema = await response.json();
  return manifest.tools
    .filter((tool) => tool.available !== false)
    .map((tool) => ({
      type: 'function',
      function: {
        name: toApiName(tool.name),
        description: tool.summary,
        parameters: inlineSchema(tool, schema),
      },
    }));
}

async function complete(messages, tools) {
  const response = await fetch(`${BASE_URL}/chat/completions`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      authorization: `Bearer ${process.env.OPENAI_API_KEY}`,
    },
    body: JSON.stringify({ model: MODEL, messages, tools, tool_choice: 'auto', temperature: 0 }),
  });
  if (!response.ok) {
    throw new Error(`model call failed: ${response.status} ${await response.text()}`);
  }
  return response.json();
}

export async function solve(task, context) {
  if (!process.env.OPENAI_API_KEY) {
    throw new Error('OPENAI_API_KEY is not set');
  }
  const tools = await loadTools(context);
  const messages = [
    {
      role: 'system',
      content:
        'You operate an ICE3 Studio modelling kernel through JSON-RPC tools. ' +
        'Read before you write. Propose edits through change sets; a proposal is not committed. ' +
        'Stop as soon as the goal is met.',
    },
    { role: 'user', content: task.goal },
  ];

  for (let turn = 0; turn < MAX_TURNS; turn += 1) {
    const body = await complete(messages, tools);
    const message = body.choices?.[0]?.message;
    if (!message) throw new Error('model returned no message');
    messages.push(message);

    const calls = message.tool_calls ?? [];
    if (calls.length === 0) return message.content;

    for (const call of calls) {
      let result;
      try {
        result = await context.client.call(
          fromApiName(call.function.name),
          JSON.parse(call.function.arguments || '{}'),
        );
      } catch (error) {
        result = { error: error.message };
      }
      messages.push({
        role: 'tool',
        tool_call_id: call.id,
        content: JSON.stringify(result).slice(0, 8000),
      });
    }
  }
  throw new Error(`goal not met within ${MAX_TURNS} turns`);
}
