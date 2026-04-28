export type ToolFormat = 'yaml' | 'xml' | 'json';

export interface CapturedToolCall {
  name: string;
  callId: string | null;
  args: Record<string, any>;
  raw: string;
  format: ToolFormat;
}

function normalizeToolText(raw: string): string {
  return raw
    .replace(/\\u003[cC]/g, '<')
    .replace(/\\u003[eE]/g, '>')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>');
}

function countIndent(line: string): number {
  let n = 0;
  while (n < line.length && line[n] === ' ') n++;
  return n;
}

function findNextNonEmpty(lines: string[], start: number): number {
  for (let i = start; i < lines.length; i++) {
    if (lines[i].trim()) return i;
  }
  return -1;
}

function parseInlineValue(raw: string): any {
  const value = raw.trim();
  if (value === '') return '';
  if (value === 'null' || value === '~') return null;
  if (value === 'true') return true;
  if (value === 'false') return false;
  if (/^-?\d+(?:\.\d+)?$/.test(value)) return Number(value);
  if (value.startsWith('"') && value.endsWith('"')) {
    try { return JSON.parse(value); } catch { return value.slice(1, -1); }
  }
  if (value.startsWith("'") && value.endsWith("'")) {
    return value.slice(1, -1).replace(/''/g, "'");
  }
  if ((value.startsWith('[') && value.endsWith(']')) || (value.startsWith('{') && value.endsWith('}'))) {
    try { return JSON.parse(value); } catch {}
  }
  return value;
}

function parseBlockScalar(lines: string[], index: number, parentIndent: number): { value: string; nextIndex: number } {
  const collected: string[] = [];
  let i = index;
  let minIndent = Number.POSITIVE_INFINITY;

  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      collected.push('');
      i++;
      continue;
    }
    const indent = countIndent(line);
    if (indent <= parentIndent) break;
    minIndent = Math.min(minIndent, indent);
    collected.push(line);
    i++;
  }

  if (collected.length === 0) return { value: '', nextIndex: index };
  const strip = Number.isFinite(minIndent) ? minIndent : parentIndent + 2;
  return {
    value: collected.map((line) => {
      if (!line) return '';
      return line.slice(Math.min(strip, countIndent(line)));
    }).join('\n'),
    nextIndex: i,
  };
}

function parseMapping(lines: string[], index: number, indent: number): { value: Record<string, any>; nextIndex: number } {
  const obj: Record<string, any> = {};
  let i = index;

  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }
    const lineIndent = countIndent(line);
    if (lineIndent < indent) break;

    const trimmed = line.trimStart();
    if (trimmed.startsWith('- ') || trimmed.startsWith('#')) {
      i++;
      continue;
    }

    const colon = trimmed.indexOf(':');
    if (colon === -1) {
      i++;
      continue;
    }

    const key = trimmed.slice(0, colon).trim();
    const rest = trimmed.slice(colon + 1).trim();
    if (rest === '|' || rest === '>') {
      const block = parseBlockScalar(lines, i + 1, lineIndent);
      obj[key] = block.value;
      i = block.nextIndex;
      continue;
    }
    if (rest !== '') {
      obj[key] = parseInlineValue(rest);
      i++;
      continue;
    }

    const next = findNextNonEmpty(lines, i + 1);
    if (next === -1 || countIndent(lines[next]) <= lineIndent) {
      obj[key] = null;
      i++;
      continue;
    }

    if (lines[next].trimStart().startsWith('- ')) {
      const list = parseList(lines, next, countIndent(lines[next]));
      obj[key] = list.value;
      i = list.nextIndex;
      continue;
    }

    const nested = parseMapping(lines, next, countIndent(lines[next]));
    obj[key] = nested.value;
    i = nested.nextIndex;
  }

  return { value: obj, nextIndex: i };
}

function parseList(lines: string[], index: number, indent: number): { value: any[]; nextIndex: number } {
  const arr: any[] = [];
  let i = index;

  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }
    const lineIndent = countIndent(line);
    if (lineIndent < indent) break;

    const trimmed = line.trimStart();
    if (!trimmed.startsWith('- ')) break;

    const itemText = trimmed.slice(2).trim();
    if (!itemText) {
      const next = findNextNonEmpty(lines, i + 1);
      if (next === -1 || countIndent(lines[next]) <= lineIndent) {
        arr.push(null);
        i++;
        continue;
      }
      if (lines[next].trimStart().startsWith('- ')) {
        const nestedList = parseList(lines, next, countIndent(lines[next]));
        arr.push(nestedList.value);
        i = nestedList.nextIndex;
        continue;
      }
      const nestedMap = parseMapping(lines, next, countIndent(lines[next]));
      arr.push(nestedMap.value);
      i = nestedMap.nextIndex;
      continue;
    }

    const colon = itemText.indexOf(':');
    if (colon === -1) {
      arr.push(parseInlineValue(itemText));
      i++;
      continue;
    }

    const key = itemText.slice(0, colon).trim();
    const rest = itemText.slice(colon + 1).trim();
    const itemObj: Record<string, any> = {};
    if (rest === '|' || rest === '>') {
      const block = parseBlockScalar(lines, i + 1, lineIndent);
      itemObj[key] = block.value;
      i = block.nextIndex;
    } else if (rest !== '') {
      itemObj[key] = parseInlineValue(rest);
      i++;
    } else {
      itemObj[key] = null;
      i++;
    }

    const next = findNextNonEmpty(lines, i);
    if (next !== -1 && countIndent(lines[next]) > lineIndent) {
      const nested = lines[next].trimStart().startsWith('- ')
        ? parseList(lines, next, countIndent(lines[next]))
        : parseMapping(lines, next, countIndent(lines[next]));
      Object.assign(itemObj, nested.value);
      i = nested.nextIndex;
    }

    arr.push(itemObj);
  }

  return { value: arr, nextIndex: i };
}

function normalizeToolCallObject(candidate: any, raw: string, format: ToolFormat): CapturedToolCall | null {
  const source = candidate?.tool_call ?? candidate;
  if (!source || typeof source !== 'object') return null;

  const name = source.name;
  if (typeof name !== 'string' || !name.trim()) return null;

  const callId = typeof source.callId === 'string'
    ? source.callId
    : typeof source.call_id === 'string'
      ? source.call_id
      : null;

  const argsSource = source.args ?? source.arguments ?? {};
  const args = argsSource && typeof argsSource === 'object' && !Array.isArray(argsSource)
    ? argsSource
    : {};

  return { name, callId, args, raw, format };
}

function parseYamlToolCall(raw: string): CapturedToolCall | null {
  const lines = raw.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n');
  const startIndex = lines.findIndex((line) => /^\s*tool_call\s*:\s*$/.test(line) || /^\s*name\s*:/.test(line));
  if (startIndex === -1) return null;

  const isWrapper = /^\s*tool_call\s*:\s*$/.test(lines[startIndex].trim());
  const baseIndent = countIndent(lines[startIndex]) + (isWrapper ? 2 : 0);
  const parsed = parseMapping(lines, isWrapper ? startIndex + 1 : startIndex, baseIndent);
  const rawSegment = lines.slice(startIndex, parsed.nextIndex).join('\n').trim();
  return normalizeToolCallObject(parsed.value, rawSegment || raw.trim(), 'yaml');
}

function parseXmlToolCall(raw: string): CapturedToolCall | null {
  const nameMatch = raw.match(/^<tool\s+name="([^"]+)"(?:\s+call_id="([^"]+)")?/);
  if (!nameMatch) return null;
  const args: Record<string, string> = {};
  const paramRe = /<parameter\s+name="([^"]+)">([\s\S]*?)<\/parameter>/g;
  let m;
  while ((m = paramRe.exec(raw)) !== null) args[m[1]] = m[2];
  return {
    name: nameMatch[1],
    callId: nameMatch[2] || null,
    args,
    raw,
    format: 'xml',
  };
}

function tryParseToolJSON(raw: string): CapturedToolCall | null {
  const normalized = raw.trim();
  if (!normalized) return null;

  try {
    return normalizeToolCallObject(JSON.parse(normalized), raw, 'json');
  } catch {}

  try {
    let result = '';
    let inString = false;
    let escaped = false;
    for (let i = 0; i < normalized.length; i++) {
      const ch = normalized[i];
      if (escaped) {
        result += ch;
        escaped = false;
        continue;
      }
      if (ch === '\\') {
        result += ch;
        escaped = true;
        continue;
      }
      if (ch === '"') {
        if (!inString) {
          inString = true;
          result += ch;
          continue;
        }
        let j = i + 1;
        while (j < normalized.length && normalized[j] === ' ') j++;
        const next = normalized[j];
        if (next === ':' || next === ',' || next === '}' || next === ']') {
          inString = false;
          result += ch;
        } else {
          result += '\\"';
        }
        continue;
      }
      result += ch;
    }
    return normalizeToolCallObject(JSON.parse(result), raw, 'json');
  } catch {}

  return null;
}

function stripMarkdownFence(raw: string): string {
  const trimmed = raw.trim();
  const fence = trimmed.match(/^```(?:ya?ml)?[^\n]*\n([\s\S]*?)\n```$/i);
  if (fence) return fence[1].trim();
  return trimmed;
}

function parseToolCallSegment(raw: string): CapturedToolCall | null {
  const text = stripMarkdownFence(normalizeToolText(raw));
  if (!text) return null;
  if (text.startsWith('<tool')) return parseXmlToolCall(text) || tryParseToolJSON(text);
  if (text.includes('tool_call:') || /^\s*name\s*:/.test(text)) return parseYamlToolCall(text) || tryParseToolJSON(text);
  if (text.startsWith('{') || text.startsWith('[')) return tryParseToolJSON(text);
  return null;
}

function isInsideSpan(index: number, spans: Array<{ start: number; end: number }>): boolean {
  return spans.some((span) => index >= span.start && index < span.end);
}

function toolCallKey(call: CapturedToolCall): string {
  if (call.callId) return `${call.name}:${call.callId}`;
  return `${call.format}:${stripMarkdownFence(call.raw).replace(/\s+/g, ' ').trim()}`;
}

function findYamlToolCallSegments(text: string, ignoredSpans: Array<{ start: number; end: number }>): Array<{ index: number; raw: string }> {
  const lines = text.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n');
  const offsets: number[] = [];
  let cursor = 0;
  for (const line of lines) {
    offsets.push(cursor);
    cursor += line.length + 1;
  }

  const starts: number[] = [];
  for (let i = 0; i < lines.length; i++) {
    const offset = offsets[i] ?? 0;
    if (!isInsideSpan(offset, ignoredSpans) && /^\s*tool_call\s*:\s*$/.test(lines[i])) starts.push(i);
  }

  return starts.map((startLine, position) => {
    const endLine = starts[position + 1] ?? lines.length;
    return {
      index: offsets[startLine] ?? 0,
      raw: lines.slice(startLine, endLine).join('\n').trim(),
    };
  }).filter((segment) => segment.raw.length > 0);
}

export function extractToolCallsFromText(text: string): CapturedToolCall[] {
  const normalized = normalizeToolText(text);
  const matches: Array<{ index: number; call: CapturedToolCall }> = [];
  const seen = new Set<string>();
  const fencedSpans: Array<{ start: number; end: number }> = [];

  const addCall = (call: CapturedToolCall | null, index: number) => {
    if (!call) return;
    const key = toolCallKey(call);
    if (seen.has(key)) return;
    seen.add(key);
    matches.push({ index, call });
  };

  const fenceRe = /```(?:ya?ml)?[^\n]*\n([\s\S]*?)```/gi;
  let match: RegExpExecArray | null;
  while ((match = fenceRe.exec(normalized)) !== null) {
    const fenceStart = match.index ?? 0;
    fencedSpans.push({ start: fenceStart, end: fenceStart + match[0].length });

    const yamlSegments = findYamlToolCallSegments(match[1], []);
    if (yamlSegments.length > 0) {
      for (const segment of yamlSegments) {
        addCall(parseToolCallSegment(segment.raw), fenceStart + segment.index);
      }
    } else {
      const call = parseToolCallSegment(match[1]);
      if (call) {
        call.raw = match[0];
        addCall(call, fenceStart);
      }
    }
  }

  const xmlRe = /<tool(?:\s[^>]*)?>[\s\S]*?<\/tool(?:_call)?>/gi;
  while ((match = xmlRe.exec(normalized)) !== null) {
    addCall(parseToolCallSegment(match[0]), match.index ?? 0);
  }

  for (const segment of findYamlToolCallSegments(normalized, fencedSpans)) {
    addCall(parseToolCallSegment(segment.raw), segment.index);
  }

  if (!matches.length) addCall(parseToolCallSegment(normalized), 0);

  matches.sort((a, b) => a.index - b.index);
  return matches.map(({ call }) => call);
}
