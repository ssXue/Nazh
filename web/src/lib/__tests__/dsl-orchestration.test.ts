import { describe, expect, it } from 'vitest';

/**
 * DSL 编排工具函数测试。
 *
 * 测试纯函数逻辑：JSON 提取、YAML 解析辅助。
 * 不依赖 Tauri IPC，所以可以在 Node 环境下运行。
 */

// 复制 tauri.ts 中 extract_json_from_ai_response 的等价逻辑
function extractJsonFromAiResponse(content: string): string {
  const trimmed = content.trim();
  // 去掉 markdown code fence
  if (trimmed.startsWith('```json')) {
    const inner = trimmed.slice(7);
    if (inner.endsWith('```')) {
      return inner.slice(0, -3).trim();
    }
    return inner.trim();
  }
  if (trimmed.startsWith('```')) {
    const inner = trimmed.slice(3);
    if (inner.endsWith('```')) {
      return inner.slice(0, -3).trim();
    }
    return inner.trim();
  }
  return trimmed;
}

describe('extractJsonFromAiResponse', () => {
  it('returns plain JSON unchanged', () => {
    const input = '{"workflowYaml": "id: test", "warnings": []}';
    expect(extractJsonFromAiResponse(input)).toBe(input);
  });

  it('strips ```json fence', () => {
    const input = '```json\n{"workflowYaml": "id: test"}\n```';
    expect(extractJsonFromAiResponse(input)).toBe('{"workflowYaml": "id: test"}');
  });

  it('strips bare ``` fence', () => {
    const input = '```\n{"workflowYaml": "id: test"}\n```';
    expect(extractJsonFromAiResponse(input)).toBe('{"workflowYaml": "id: test"}');
  });

  it('handles whitespace around fenced content', () => {
    const input = '  ```json\n  {"a": 1}  \n```  ';
    expect(extractJsonFromAiResponse(input)).toBe('{"a": 1}');
  });

  it('handles JSON with nested objects', () => {
    const obj = { workflowYaml: 'id: test\nstates: {}', uncertainties: [{ fieldPath: 'x', guessedValue: '1', reason: 'guess' }] };
    const json = JSON.stringify(obj);
    const fenced = `\`\`\`json\n${json}\n\`\`\``;
    const extracted = extractJsonFromAiResponse(fenced);
    expect(JSON.parse(extracted)).toEqual(obj);
  });
});

describe('AI proposal JSON parsing', () => {
  it('parses camelCase proposal', () => {
    const json = JSON.stringify({
      workflowYaml: 'id: test\nversion: "1.0.0"',
      uncertainties: [],
      warnings: ['test warning'],
    });
    const parsed = JSON.parse(json);
    expect(parsed.workflowYaml).toBe('id: test\nversion: "1.0.0"');
    expect(parsed.warnings).toHaveLength(1);
  });

  it('parses snake_case proposal with fallback', () => {
    const json = JSON.stringify({
      workflow_yaml: 'id: test',
      uncertainties: [],
      warnings: [],
    });
    const parsed = JSON.parse(json);
    const yaml = parsed.workflowYaml ?? parsed.workflow_yaml ?? '';
    expect(yaml).toBe('id: test');
  });
});
