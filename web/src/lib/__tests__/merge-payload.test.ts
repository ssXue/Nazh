import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mergePullPayload } from '../merge-payload';

const __dirname = dirname(fileURLToPath(import.meta.url));

describe('mergePullPayload — fixture parity with Rust merge_payload', () => {
  const raw = readFileSync(
    resolve(__dirname, '../../../../tests/fixtures/mixed_input_merge.jsonc'),
    'utf8',
  );
  const stripped = raw
    .split('\n')
    .map((line) => {
      const idx = line.indexOf('//');
      return idx >= 0 ? line.slice(0, idx) : line;
    })
    .join('\n');
  const cases = JSON.parse(stripped) as Array<{
    name: string;
    exec_payload: unknown;
    data_values: Record<string, unknown>;
    merged: unknown;
  }>;

  it.each(cases)('$name', (c) => {
    expect(mergePullPayload(c.exec_payload, c.data_values)).toEqual(c.merged);
  });
});
