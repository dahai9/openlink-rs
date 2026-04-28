import { extractToolCallsFromText } from '../shared/toolcall';

(function() {
  console.log('[OpenLink] 插件已加载');
  const originalFetch = window.fetch;
  let buffer = '';

  // Global dedup: keyed by conversation ID extracted from URL
  const processedByConv = new Map<string, Set<string>>();

  function getConvId(): string {
    // Claude: /chat/<id>, ChatGPT: /c/<id>, DeepSeek: ?id=<id> or path
    const m = location.pathname.match(/\/(?:chat|c)\/([^/?#]+)/) ||
              location.search.match(/[?&]id=([^&]+)/);
    return m ? m[1] : '__default__';
  }

  function getProcessed(): Set<string> {
    const id = getConvId();
    if (!processedByConv.has(id)) processedByConv.set(id, new Set());
    return processedByConv.get(id)!;
  }

  function emitToolCalls(flushFinal: boolean) {
    const processed = getProcessed();
    const calls = extractToolCallsFromText(buffer);
    for (const toolCall of calls) {
      const wrapped = toolCall.raw.trimStart().startsWith('```') || toolCall.raw.trimStart().startsWith('<tool');
      if (!flushFinal && !wrapped) continue;
      if (processed.has(toolCall.raw)) continue;
      processed.add(toolCall.raw);
      window.postMessage({type: 'TOOL_CALL', data: toolCall}, '*');
      buffer = buffer.replace(toolCall.raw, '');
    }
  }

  window.fetch = function(...args) {
    const decoder = new TextDecoder();
    return originalFetch.apply(this, args).then(async response => {
      const reader = response.body!.getReader();
      const stream = new ReadableStream({
        async start(controller) {
          while (true) {
            const {done, value} = await reader.read();
            if (done) {
              emitToolCalls(true);
              buffer = '';
              break;
            }

            const text = decoder.decode(value, { stream: true });
            buffer += text;
            emitToolCalls(false);
            controller.enqueue(value);
          }
          controller.close();
        }
      });

      return new Response(stream, {
        headers: response.headers,
        status: response.status
      });
    });
  };
})();
