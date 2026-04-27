import { extractToolCallsFromText } from '../shared/toolcall';

(function() {
  console.log('[OpenLink] 插件已加载');
  const originalFetch = window.fetch;
  const originalXhrSend = XMLHttpRequest.prototype.send;

  // Global dedup: keyed by conversation ID extracted from URL
  const processedByConv = new Map<string, Set<string>>();
  const xhrStateByInstance = new WeakMap<XMLHttpRequest, { buffer: string; lastLength: number; processed: Set<string> }>();

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

  function normalizeToolText(raw: string): string {
    return raw
      .replace(/\\u003[cC]/g, '<')
      .replace(/\\u003[eE]/g, '>')
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>');
  }

  function emitToolCallsFromBuffer(rawBuffer: string, processed: Set<string>): string {
    let buffer = normalizeToolText(rawBuffer);
    const calls = extractToolCallsFromText(buffer);
    for (const call of calls) {
      if (processed.has(call.raw)) continue;
      processed.add(call.raw);
      window.postMessage({
        type: 'TOOL_CALL',
        data: call,
        __openlinkSource: 'injected',
        __openlinkRaw: call.raw,
      }, '*');
      buffer = buffer.replace(call.raw, '');
    }

    return buffer;
  }

  async function observeResponseBody(response: Response, processed: Set<string>): Promise<void> {
    const body = response.body;
    if (!body) return;

    const reader = body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!value) continue;
      buffer += decoder.decode(value, { stream: true });
      buffer = emitToolCallsFromBuffer(buffer, processed);
    }

    // Flush decoder tail and run one final extraction pass.
    buffer += decoder.decode();
    emitToolCallsFromBuffer(buffer, processed);
  }

  function observeXhrBody(xhr: XMLHttpRequest, processed: Set<string>): void {
    if (xhrStateByInstance.has(xhr)) return;

    const state = { buffer: '', lastLength: 0, processed };
    xhrStateByInstance.set(xhr, state);

    const flush = () => {
      try {
        if (xhr.responseType && xhr.responseType !== 'text') return;
        const text = xhr.responseText || '';
        if (text.length < state.lastLength) {
          state.buffer = '';
          state.lastLength = 0;
        }
        const chunk = text.slice(state.lastLength);
        state.lastLength = text.length;
        if (!chunk) return;
        state.buffer += chunk;
        state.buffer = emitToolCallsFromBuffer(state.buffer, state.processed);
      } catch {
        // Ignore JSON/blob/text-unavailable responses and keep page behavior unchanged.
      }
    };

    xhr.addEventListener('progress', flush);
    xhr.addEventListener('readystatechange', () => {
      if (xhr.readyState === XMLHttpRequest.LOADING || xhr.readyState === XMLHttpRequest.DONE) flush();
    });
    xhr.addEventListener('loadend', () => {
      flush();
      xhrStateByInstance.delete(xhr);
    });
  }

  window.fetch = function(...args) {
    return originalFetch.apply(this, args).then(response => {
      try {
        // Observe a cloned stream to avoid altering the page's original response lifecycle.
        const cloned = response.clone();
        void observeResponseBody(cloned, getProcessed());
      } catch {
        // Ignore clone/read failures and keep page behavior unchanged.
      }
      return response;
    });
  };

  XMLHttpRequest.prototype.send = function(body) {
    try {
      observeXhrBody(this, getProcessed());
    } catch {
      // Keep the page flow intact even if the hook fails to attach.
    }
    return originalXhrSend.call(this, body);
  };
})();
