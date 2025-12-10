import type { ElementInfo, A11yNodeInfo, SelectionMode, NavigationType } from '../lib/types';

const isTopFrame = window === window.top;
const EXCLUDED_TAGS = ['SCRIPT', 'STYLE', 'NOSCRIPT', 'SVG', 'TEMPLATE'];

interface State {
  selectionMode: SelectionMode | null;
  selectionFilter: string | null;
  selectedElements: Element[];
  iframeActive: boolean;
}

const state: State = {
  selectionMode: null,
  selectionFilter: null,
  selectedElements: [],
  iframeActive: false,
};

let hoverOverlay: HTMLElement | null = null;
let tooltip: HTMLElement | null = null;

const inputState = {
  pending: new WeakMap<Element, { value: string; timer: number | null }>(),
  lastSent: new WeakMap<Element, string>(),
  activeElement: null as Element | null,
};

const scrollState = {
  timer: null as number | null,
  lastY: 0,
  lastUrl: '',
  pending: false,
};

function extractText(element: Element, maxLength: number): string | undefined {
  if (EXCLUDED_TAGS.includes(element.tagName)) return undefined;

  const directText = Array.from(element.childNodes)
    .filter((n) => n.nodeType === Node.TEXT_NODE)
    .map((n) => n.textContent?.trim())
    .filter(Boolean)
    .join(' ');

  if (directText) return directText.slice(0, maxLength);

  const clone = element.cloneNode(true) as Element;
  clone.querySelectorAll(EXCLUDED_TAGS.join(',').toLowerCase()).forEach((el) => el.remove());
  const text = clone.textContent?.trim();
  return text?.slice(0, maxLength) || undefined;
}

chrome.runtime.onMessage.addListener((message, _, sendResponse) => {
  switch (message.type) {
    case 'start_selection':
      startSelection(message.mode, message.filter);
      break;
    case 'cancel_selection':
      cancelSelection();
      break;
    case 'highlight':
      highlightElement(message.selector, message.color);
      break;
    case 'clear_highlight':
      clearAllOverlays();
      break;
    case 'get_snapshot':
      if (isTopFrame) sendResponse(captureSnapshot(message.verbose));
      break;
    case 'get_a11y_tree':
      if (isTopFrame) sendResponse(getA11yTree());
      break;
    case 'start_recording':
      sendResponse({ ok: true });
      break;
    case 'stop_recording':
      sendResponse({ ok: true });
      break;
  }
  return true;
});

if (isTopFrame) {
  window.addEventListener('message', handleIframeMessage);
}

function handleIframeMessage(e: MessageEvent): void {
  const data = e.data;
  if (!data || typeof data.type !== 'string') return;

  if (data.type === '__cdtcli_iframe_hover__') {
    state.iframeActive = true;
    removeHoverOverlay();
    updateTooltip(null);
  }

  if (data.type === '__cdtcli_iframe_leave__') {
    state.iframeActive = false;
  }

  if (data.type === '__cdtcli_iframe_selected__') {
    state.iframeActive = false;
    removeHoverOverlay();
    updateTooltip(null);
    cancelSelection();

    const { elementInfo, bounds } = data;
    const iframes = document.querySelectorAll('iframe');
    let sourceIframe: HTMLIFrameElement | null = null;

    for (const iframe of iframes) {
      if (iframe.contentWindow === e.source) {
        sourceIframe = iframe;
        break;
      }
    }

    if (sourceIframe) {
      const iframeRect = sourceIframe.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;

      const adjustedBounds = {
        x: Math.round((bounds.x + iframeRect.left) * dpr),
        y: Math.round((bounds.y + iframeRect.top) * dpr),
        width: Math.round(bounds.width * dpr),
        height: Math.round(bounds.height * dpr),
      };

      const iframeSelector = generateSelector(sourceIframe);
      const fullSelector = `${iframeSelector} >> ${elementInfo.selector}`;

      chrome.runtime.sendMessage({
        type: 'capture_element_screenshot',
        bounds: adjustedBounds,
        elementInfo: {
          ...elementInfo,
          selector: fullSelector,
        },
      });

      const role = elementInfo.a11y?.role || 'generic';
      const name = elementInfo.a11y?.name;
      sendToCli({
        select: {
          aria: name ? [role, name] : [role],
          css: fullSelector,
          xpath: elementInfo.xpath,
          text: elementInfo.text,
          rect: [
            Math.round(bounds.x),
            Math.round(bounds.y),
            Math.round(bounds.width),
            Math.round(bounds.height),
          ],
          url: window.location.href,
          ts: Date.now(),
        },
      });
    }
  }
}

function notifyParentHover(): void {
  if (isTopFrame) return;
  try {
    window.parent.postMessage({ type: '__cdtcli_iframe_hover__' }, '*');
  } catch {}
}

function notifyParentLeave(): void {
  if (isTopFrame) return;
  try {
    window.parent.postMessage({ type: '__cdtcli_iframe_leave__' }, '*');
  } catch {}
}

function notifyParentSelected(element: Element, info: ElementInfo): void {
  if (isTopFrame) return;
  const rect = element.getBoundingClientRect();
  try {
    window.parent.postMessage({
      type: '__cdtcli_iframe_selected__',
      elementInfo: {
        ...info,
        dimensions: `${Math.round(rect.width)} × ${Math.round(rect.height)}`,
      },
      bounds: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
    }, '*');
  } catch {}
}

window.addEventListener('click', handleClick, true);
window.addEventListener('pointerdown', handlePointerDown, true);
document.addEventListener('mousemove', handleMouseMove, true);
document.addEventListener('keydown', handleKeyDown, true);
document.addEventListener('input', handleInputChange, true);
document.addEventListener('focusin', handleFocusIn, true);
document.addEventListener('focusout', handleFocusOut, true);
document.addEventListener('scroll', handleScroll, true);
window.addEventListener('beforeunload', flushPendingEvents);

if (!isTopFrame) {
  document.addEventListener('mouseleave', handleIframeMouseLeave);
}

function handleIframeMouseLeave(): void {
  if (!state.selectionMode) return;
  removeHoverOverlay();
  notifyParentLeave();
}

function startSelection(mode: SelectionMode, filter?: string): void {
  state.selectionMode = mode;
  state.selectionFilter = filter || null;
  state.selectedElements = [];
  document.body.style.cursor = 'crosshair';
  createTooltip();
  if (isTopFrame) {
    showToast('Click to select an element. Press ESC to cancel.', 0);
  }
}

function cancelSelection(): void {
  state.selectionMode = null;
  state.selectionFilter = null;
  state.selectedElements = [];
  state.iframeActive = false;
  document.body.style.cursor = '';
  clearAllOverlays();
  if (isTopFrame) hideToast();
}

// Track last pointer event to avoid duplicate click tracking
let lastPointerTs = 0;

function handlePointerDown(e: PointerEvent): void {
  // Only track primary button (left click)
  if (e.button !== 0 || e.pointerType === 'touch') return;

  // Store timestamp to detect if click event fires
  lastPointerTs = Date.now();

  // Delay slightly to allow click event to fire first
  setTimeout(() => {
    // If more than 300ms passed without click event, track via pointerdown
    if (Date.now() - lastPointerTs >= 300) {
      trackClick(e as unknown as MouseEvent);
    }
  }, 350);
}

function handleClick(e: MouseEvent): void {
  // Reset pointer timestamp to prevent duplicate tracking
  lastPointerTs = 0;

  if (state.selectionMode) {
    handleSelectionClick(e);
    return;
  }
  trackClick(e);
}

function handleSelectionClick(e: MouseEvent): void {
  const element = document.elementFromPoint(e.clientX, e.clientY);
  if (!element || !matchesFilter(element)) return;
  if (element.tagName === 'IFRAME') return;

  e.preventDefault();
  e.stopPropagation();

  const info = getElementInfo(element);

  if (state.selectionMode === 'single') {
    if (isTopFrame) {
      captureAndNotify(element, info);
    } else {
      notifyParentSelected(element, info);
    }
    cancelSelection();
  } else if (state.selectionMode === 'multiple') {
    state.selectedElements.push(element);
    addSelectionOverlay(element);
    if (isTopFrame) {
      showToast(`Selected ${state.selectedElements.length} elements. Press Enter to confirm.`, 0);
    }
  }
}

function trackClick(e: MouseEvent): void {
  if (!isTopFrame) return;

  const target = e.target as Element;
  const info = getElementInfo(target);
  const targetInfo = buildTargetInfo(target, info);

  sendToCli({
    click: {
      aria: targetInfo.aria,
      css: targetInfo.css,
      xpath: targetInfo.xpath,
      testid: targetInfo.testid,
      text: targetInfo.text,
      rect: targetInfo.rect,
      url: window.location.href,
      ts: Date.now(),
    },
  });
}

function handleMouseMove(e: MouseEvent): void {
  if (!state.selectionMode) return;
  if (isTopFrame && state.iframeActive) return;

  const element = document.elementFromPoint(e.clientX, e.clientY);
  if (!element || element === hoverOverlay || element === tooltip) return;

  if (element.tagName === 'IFRAME') {
    removeHoverOverlay();
    if (isTopFrame) updateTooltip(null);
    return;
  }

  if (!matchesFilter(element)) {
    removeHoverOverlay();
    if (isTopFrame) updateTooltip(null);
    return;
  }

  if (!isTopFrame) {
    notifyParentHover();
  }

  showHoverOverlay(element);
  updateTooltip(element);
}

function handleKeyDown(e: KeyboardEvent): void {
  if (state.selectionMode) {
    if (e.key === 'Escape') {
      cancelSelection();
      if (isTopFrame) showToast('Selection cancelled', 2000);
      return;
    }
    if (e.key === 'Enter' && state.selectionMode === 'multiple' && state.selectedElements.length > 0) {
      if (isTopFrame) {
        for (const el of state.selectedElements) {
          const info = getElementInfo(el);
          const semanticElement = getSemanticElement(el);
          const elementRect = getElementRect(el);
          sendToCli({
            select: {
              aria: semanticElement.name ? [semanticElement.role, semanticElement.name] : [semanticElement.role],
              css: info.selector,
              xpath: info.xpath,
              text: semanticElement.text,
              rect: elementRect,
              url: window.location.href,
              ts: Date.now(),
            },
          });
        }
      }
      cancelSelection();
      return;
    }
    return;
  }

  if (!isTopFrame) return;

  const significantKeys = ['Enter', 'Tab', 'Escape'];
  if (!significantKeys.includes(e.key)) return;

  const target = e.target as Element;
  const isInputElement = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT';

  if (e.key === 'Enter' && isInputElement) {
    flushInputEvent(target);
  }

  const info = getElementInfo(target);
  const targetInfo = buildTargetInfo(target, info);

  sendToCli({
    keypress: {
      key: e.key,
      aria: targetInfo.aria,
      css: targetInfo.css,
      xpath: targetInfo.xpath,
      testid: targetInfo.testid,
      url: window.location.href,
      ts: Date.now(),
    },
  });
}

function isInputElement(el: Element): el is HTMLInputElement | HTMLTextAreaElement {
  return el.tagName === 'INPUT' || el.tagName === 'TEXTAREA';
}

function handleFocusIn(e: FocusEvent): void {
  const target = e.target as Element;
  if (isInputElement(target)) {
    inputState.activeElement = target;
  }
}

function handleFocusOut(e: FocusEvent): void {
  const target = e.target as Element;
  if (isInputElement(target)) {
    flushInputEvent(target);
    inputState.activeElement = null;
  }
}

function handleInputChange(e: Event): void {
  if (!isTopFrame) return;
  const target = e.target as HTMLInputElement | HTMLTextAreaElement;
  if (!isInputElement(target)) return;

  const pending = inputState.pending.get(target);
  if (pending?.timer) clearTimeout(pending.timer);

  const timer = setTimeout(() => {
    const p = inputState.pending.get(target);
    if (p) p.timer = null;
  }, 200) as unknown as number;

  inputState.pending.set(target, { value: target.value, timer });
}

function flushInputEvent(element: Element): void {
  if (!isInputElement(element)) return;

  const pending = inputState.pending.get(element);
  if (!pending) return;

  if (pending.timer) {
    clearTimeout(pending.timer);
    pending.timer = null;
  }

  const lastSent = inputState.lastSent.get(element);
  if (pending.value === lastSent) {
    inputState.pending.delete(element);
    return;
  }

  inputState.lastSent.set(element, pending.value);
  inputState.pending.delete(element);

  const info = getElementInfo(element);
  const targetInfo = buildTargetInfo(element, info);

  sendToCli({
    input: {
      aria: targetInfo.aria,
      css: targetInfo.css,
      xpath: targetInfo.xpath,
      testid: targetInfo.testid,
      rect: targetInfo.rect,
      value: pending.value,
      url: window.location.href,
      ts: Date.now(),
    },
  });
}

function handleScroll(): void {
  if (!isTopFrame) return;

  if (scrollState.timer) clearTimeout(scrollState.timer);
  scrollState.pending = true;

  scrollState.timer = setTimeout(() => {
    flushScrollEvent();
  }, 300) as unknown as number;
}

function flushScrollEvent(): void {
  if (!scrollState.pending) return;
  scrollState.pending = false;

  if (scrollState.timer) {
    clearTimeout(scrollState.timer);
    scrollState.timer = null;
  }

  const currentUrl = window.location.href;

  if (currentUrl !== scrollState.lastUrl) {
    scrollState.lastUrl = currentUrl;
    scrollState.lastY = window.scrollY;
    if (window.scrollY === 0) return;
  }

  const deltaY = Math.abs(window.scrollY - scrollState.lastY);
  if (deltaY < 50) return;

  scrollState.lastY = window.scrollY;
  sendToCli({
    scroll: {
      x: window.scrollX,
      y: window.scrollY,
      url: currentUrl,
      ts: Date.now(),
    },
  });
}

function flushPendingEvents(): void {
  if (inputState.activeElement) {
    flushInputEvent(inputState.activeElement);
  }
  flushScrollEvent();
}

function matchesFilter(element: Element): boolean {
  if (!state.selectionFilter) return true;
  try {
    return element.matches(state.selectionFilter);
  } catch {
    return true;
  }
}

async function captureAndNotify(element: Element, info: ElementInfo): Promise<void> {
  const rect = element.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const semanticElement = getSemanticElement(element);
  const elementRect = getElementRect(element);

  await chrome.runtime.sendMessage({
    type: 'capture_element_screenshot',
    bounds: {
      x: Math.round(rect.x * dpr),
      y: Math.round(rect.y * dpr),
      width: Math.round(rect.width * dpr),
      height: Math.round(rect.height * dpr),
    },
    elementInfo: {
      selector: info.selector,
      tagName: info.tagName,
      className: (element as HTMLElement).className || undefined,
      id: element.id || undefined,
      text: info.text,
      dimensions: `${Math.round(rect.width)} × ${Math.round(rect.height)}`,
    },
  });

  const target = semanticElement.name ? [semanticElement.role, semanticElement.name] : [semanticElement.role];

  sendToCli({
    select: {
      aria: target,
      css: info.selector,
      xpath: info.xpath,
      text: semanticElement.text,
      rect: elementRect,
      url: window.location.href,
      ts: Date.now(),
    },
  });
}

interface TargetInfo {
  aria?: [string, string?];
  css: string;
  xpath?: string;
  testid?: string;
  text?: string;
  rect: [number, number, number, number];
}

function buildTargetInfo(element: Element, info: ElementInfo): TargetInfo {
  const semantic = getSemanticElement(element);
  const rect = getElementRect(element);
  const testid = info.attributes?.['data-testid'];

  const hasAriaValue = semantic.role !== 'generic' || semantic.name;
  const aria = hasAriaValue
    ? (semantic.name ? [semantic.role, semantic.name] as [string, string] : [semantic.role] as [string])
    : undefined;

  return {
    aria,
    css: info.selector,
    xpath: info.xpath,
    testid,
    text: semantic.text,
    rect,
  };
}

function showToast(text: string, duration: number): void {
  hideToast();
  const toast = document.createElement('div');
  toast.id = '__cdtcli_toast__';
  toast.textContent = text;
  toast.style.cssText =
    'position:fixed;bottom:24px;left:50%;transform:translateX(-50%);padding:12px 24px;background:#1a1a1a;color:#fff;font:14px/1.4 system-ui,sans-serif;border-radius:8px;z-index:2147483647;box-shadow:0 4px 16px rgba(0,0,0,0.3)';
  document.body.appendChild(toast);
  if (duration > 0) setTimeout(() => toast.remove(), duration);
}

function hideToast(): void {
  document.getElementById('__cdtcli_toast__')?.remove();
}

function showHoverOverlay(element: Element): void {
  const uid = generateUid(element);
  if (hoverOverlay?.dataset.uid === uid) return;
  removeHoverOverlay();

  const rect = element.getBoundingClientRect();
  hoverOverlay = document.createElement('div');
  hoverOverlay.dataset.uid = uid;
  hoverOverlay.style.cssText = `position:fixed;top:${rect.top}px;left:${rect.left}px;width:${rect.width}px;height:${rect.height}px;border:2px solid #0066ff;background:rgba(0,102,255,0.1);pointer-events:none;z-index:2147483646;box-sizing:border-box`;
  document.body.appendChild(hoverOverlay);
}

function removeHoverOverlay(): void {
  hoverOverlay?.remove();
  hoverOverlay = null;
}

function createTooltip(): void {
  if (tooltip) return;
  tooltip = document.createElement('div');
  tooltip.style.cssText =
    'position:fixed;padding:6px 8px;background:#1a1a1a;color:#fff;font:12px/1.4 system-ui,sans-serif;border-radius:4px;z-index:2147483647;pointer-events:none;opacity:0;max-width:400px;box-shadow:0 2px 8px rgba(0,0,0,0.3)';
  document.body.appendChild(tooltip);
}

function updateTooltip(element: Element | null): void {
  if (!tooltip) return;
  if (!element) {
    tooltip.style.opacity = '0';
    return;
  }

  const rect = element.getBoundingClientRect();
  const tagName = element.tagName.toLowerCase();
  const className = (element as HTMLElement).className;
  const id = element.id;

  let label = tagName;
  if (id) label += `#${id}`;
  if (className && typeof className === 'string') {
    const classes = className.trim().split(/\s+/).slice(0, 2).join('.');
    if (classes) label += `.${classes}`;
  }

  tooltip.innerHTML = `
    <div style="color:#e8912d;font-family:monospace;font-size:11px">${label}</div>
    <div style="color:#888;font-size:10px;margin-top:2px">${Math.round(rect.width)} × ${Math.round(rect.height)}</div>
  `;

  let top = rect.bottom + 6;
  let left = rect.left;
  if (top + 50 > window.innerHeight) top = rect.top - 50;
  if (left + 200 > window.innerWidth) left = window.innerWidth - 210;

  tooltip.style.top = `${Math.max(4, top)}px`;
  tooltip.style.left = `${Math.max(4, left)}px`;
  tooltip.style.opacity = '1';
}

function addSelectionOverlay(element: Element): void {
  const rect = element.getBoundingClientRect();
  const overlay = document.createElement('div');
  overlay.className = '__cdtcli_selection__';
  overlay.style.cssText = `position:fixed;top:${rect.top}px;left:${rect.left}px;width:${rect.width}px;height:${rect.height}px;border:2px solid #00cc66;background:rgba(0,204,102,0.15);pointer-events:none;z-index:2147483645;box-sizing:border-box`;
  document.body.appendChild(overlay);
}

function highlightElement(selector: string, color?: string): void {
  try {
    const element = document.querySelector(selector);
    if (!element) return;

    const rect = element.getBoundingClientRect();
    const overlay = document.createElement('div');
    overlay.className = '__cdtcli_highlight__';
    overlay.style.cssText = `position:fixed;top:${rect.top}px;left:${rect.left}px;width:${rect.width}px;height:${rect.height}px;border:3px solid ${color || '#ff6600'};background:${color ? color + '20' : 'rgba(255,102,0,0.1)'};pointer-events:none;z-index:2147483644;box-sizing:border-box`;
    document.body.appendChild(overlay);
  } catch {}
}

function clearAllOverlays(): void {
  document.querySelectorAll('.__cdtcli_selection__,.__cdtcli_highlight__').forEach((el) => el.remove());
  removeHoverOverlay();
  tooltip?.remove();
  tooltip = null;
}

function getElementInfo(element: Element): ElementInfo {
  const rect = element.getBoundingClientRect();
  const style = window.getComputedStyle(element);

  return {
    uid: generateUid(element),
    selector: generateSelector(element),
    xpath: generateXPath(element),
    tagName: element.tagName.toLowerCase(),
    boundingBox: {
      x: rect.x + window.scrollX,
      y: rect.y + window.scrollY,
      width: rect.width,
      height: rect.height,
    },
    text: getVisibleText(element),
    attributes: getRelevantAttributes(element),
    computed: {
      display: style.display,
      visibility: style.visibility,
      isVisible: isElementVisible(element),
    },
    a11y: {
      role: element.getAttribute('role') || getImplicitRole(element),
      name: getAccessibleName(element) || undefined,
    },
  };
}

function generateUid(element: Element): string {
  const path: number[] = [];
  let current: Element | null = element;

  while (current && current !== document.documentElement) {
    const parent = current.parentElement;
    if (parent) path.unshift(Array.from(parent.children).indexOf(current));
    current = parent;
  }

  return path.join('.');
}

function generateSelector(element: Element): string {
  if (element.id) {
    const escapedId = CSS.escape(element.id);
    try {
      if (document.querySelectorAll(`#${escapedId}`).length === 1) {
        return `#${escapedId}`;
      }
    } catch {}
  }

  const testId = element.getAttribute('data-testid');
  if (testId) {
    const selector = `[data-testid="${CSS.escape(testId)}"]`;
    try {
      if (document.querySelectorAll(selector).length === 1) {
        return selector;
      }
    } catch {}
  }

  const uniqueClassSelector = findUniqueClassSelector(element);
  if (uniqueClassSelector) {
    return uniqueClassSelector;
  }

  return generatePathSelector(element);
}

function findUniqueClassSelector(element: Element): string | null {
  const tagName = element.tagName.toLowerCase();
  const classList = Array.from(element.classList).filter(
    (c) => !c.match(/^(active|hover|focus|disabled|hidden|visible|open|closed|selected|checked)$/i)
  );

  if (classList.length === 0) return null;

  for (const cls of classList) {
    const selector = `${tagName}.${CSS.escape(cls)}`;
    try {
      if (document.querySelectorAll(selector).length === 1) {
        return selector;
      }
    } catch {}
  }

  if (classList.length >= 2) {
    for (let i = 0; i < classList.length; i++) {
      for (let j = i + 1; j < classList.length; j++) {
        const selector = `${tagName}.${CSS.escape(classList[i])}.${CSS.escape(classList[j])}`;
        try {
          if (document.querySelectorAll(selector).length === 1) {
            return selector;
          }
        } catch {}
      }
    }
  }

  return null;
}

function generatePathSelector(element: Element): string {
  const parts: string[] = [];
  let current: Element | null = element;

  while (current && current !== document.body && current !== document.documentElement) {
    let selector = current.tagName.toLowerCase();

    if (current.id) {
      const escapedId = CSS.escape(current.id);
      parts.unshift(`#${escapedId}`);
      break;
    }

    const classList = Array.from(current.classList).filter(
      (c) => !c.match(/^(active|hover|focus|disabled|hidden|visible|open|closed|selected|checked)$/i)
    );
    if (classList.length > 0) {
      selector += '.' + classList.slice(0, 2).map((c) => CSS.escape(c)).join('.');
    }

    const parent = current.parentElement;
    if (parent) {
      const siblings = Array.from(parent.children).filter((el) => {
        if (el.tagName !== current!.tagName) return false;
        if (classList.length > 0) {
          return classList.every((c) => el.classList.contains(c));
        }
        return true;
      });

      if (siblings.length > 1) {
        selector += `:nth-child(${Array.from(parent.children).indexOf(current) + 1})`;
      }
    }

    parts.unshift(selector);
    current = parent;
  }

  return parts.join(' > ');
}

function generateXPath(element: Element): string {
  const parts: string[] = [];
  let current: Node | null = element;

  while (current && current.nodeType === Node.ELEMENT_NODE) {
    const el = current as Element;
    let part = el.tagName.toLowerCase();

    if (el.id) {
      return `//*[@id="${el.id}"]${parts.length ? '/' + parts.join('/') : ''}`;
    }

    const parent = el.parentNode;
    if (parent) {
      const siblings = Array.from(parent.childNodes).filter(
        (n) => n.nodeType === Node.ELEMENT_NODE && (n as Element).tagName === el.tagName
      );
      if (siblings.length > 1) {
        part += `[${siblings.indexOf(el) + 1}]`;
      }
    }

    parts.unshift(part);
    current = parent;
  }

  return '/' + parts.join('/');
}

function getVisibleText(element: Element): string | undefined {
  return extractText(element, 100);
}

function getRelevantAttributes(element: Element): Record<string, string> | undefined {
  const attrs: Record<string, string> = {};
  const relevantAttrs = [
    'href',
    'src',
    'alt',
    'title',
    'placeholder',
    'name',
    'type',
    'value',
    'data-testid',
    'aria-label',
  ];

  for (const name of relevantAttrs) {
    const value = element.getAttribute(name);
    if (value) attrs[name] = value.slice(0, 200);
  }

  return Object.keys(attrs).length > 0 ? attrs : undefined;
}

function isElementVisible(element: Element): boolean {
  const style = window.getComputedStyle(element);
  if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') {
    return false;
  }

  const rect = element.getBoundingClientRect();
  return rect.width > 0 && rect.height > 0;
}

function isInViewport(element: Element): boolean {
  const rect = element.getBoundingClientRect();
  return rect.top < window.innerHeight && rect.bottom > 0 && rect.left < window.innerWidth && rect.right > 0;
}

const IMPLICIT_ROLES: Record<string, string> = {
  a: 'link',
  button: 'button',
  input: 'textbox',
  select: 'combobox',
  textarea: 'textbox',
  img: 'img',
  nav: 'navigation',
  main: 'main',
  header: 'banner',
  footer: 'contentinfo',
  article: 'article',
  section: 'region',
  aside: 'complementary',
  form: 'form',
};

function getImplicitRole(element: Element): string {
  return IMPLICIT_ROLES[element.tagName.toLowerCase()] || 'generic';
}

function getAccessibleName(element: Element): string | null {
  const ariaLabel = element.getAttribute('aria-label');
  if (ariaLabel) return ariaLabel;

  const ariaLabelledBy = element.getAttribute('aria-labelledby');
  if (ariaLabelledBy) {
    const labelEl = document.getElementById(ariaLabelledBy);
    if (labelEl) return labelEl.textContent?.trim().slice(0, 100) || null;
  }

  const tagName = element.tagName;
  if (tagName === 'IMG') return (element as HTMLImageElement).alt || null;
  if (tagName === 'INPUT') {
    const input = element as HTMLInputElement;
    if (input.placeholder) return input.placeholder;
    if (input.name) return input.name;
    const label = document.querySelector(`label[for="${input.id}"]`);
    if (label) return label.textContent?.trim().slice(0, 100) || null;
  }
  if (tagName === 'BUTTON' || tagName === 'A') return element.textContent?.trim().slice(0, 100) || null;

  return null;
}

interface SemanticElement {
  role: string;
  name?: string;
  text?: string;
}

function getSemanticElement(element: Element): SemanticElement {
  const role = element.getAttribute('role') || getImplicitRole(element);
  const name = getAccessibleName(element) || undefined;
  let text = extractText(element, 50);
  if (text && name && text === name) text = undefined;
  return { role, name, text };
}

function getElementRect(element: Element): [number, number, number, number] {
  const rect = element.getBoundingClientRect();
  return [
    Math.round(rect.x),
    Math.round(rect.y),
    Math.round(rect.width),
    Math.round(rect.height),
  ];
}

function captureSnapshot(verbose: boolean): object {
  const selectors = [
    'a[href]',
    'button',
    'input',
    'select',
    'textarea',
    '[onclick]',
    '[role="button"]',
    '[role="link"]',
  ];

  const elements = Array.from(document.querySelectorAll(selectors.join(',')))
    .filter((el) => isElementVisible(el) && isInViewport(el))
    .slice(0, 100)
    .map((el) => getElementInfo(el));

  return {
    type: 'page_snapshot',
    url: window.location.href,
    title: document.title,
    viewport: {
      width: window.innerWidth,
      height: window.innerHeight,
      scrollX: window.scrollX,
      scrollY: window.scrollY,
      devicePixelRatio: window.devicePixelRatio,
    },
    interactiveElements: elements,
    a11yTree: verbose ? buildA11yTree(document.body, 0) : null,
    timestamp: new Date().toISOString(),
  };
}

function getA11yTree(): object {
  return {
    type: 'a11y_tree',
    root: buildA11yTree(document.body, 0),
    timestamp: new Date().toISOString(),
  };
}

function buildA11yTree(element: Element, depth: number): A11yNodeInfo {
  const node: A11yNodeInfo = {
    uid: generateUid(element),
    role: element.getAttribute('role') || getImplicitRole(element),
    name: getAccessibleName(element) || undefined,
  };

  if (depth < 10) {
    const children = Array.from(element.children)
      .filter(isElementVisible)
      .map((c) => buildA11yTree(c, depth + 1));
    if (children.length) node.children = children;
  }

  return node;
}

function sendToCli(event: object): void {
  chrome.runtime.sendMessage({ type: 'user_action', action: event });
}

function sendNavigateEvent(navType: NavigationType, from?: string): void {
  if (!isTopFrame) return;
  sendToCli({
    navigate: {
      url: location.href,
      from,
      type: navType,
      ts: Date.now(),
    },
  });
}

if (isTopFrame) {
  window.addEventListener('load', () => sendNavigateEvent('load'));
  window.addEventListener('popstate', () => sendNavigateEvent('popState'));

  const originalPushState = history.pushState;
  history.pushState = function (...args) {
    const from = location.href;
    originalPushState.apply(this, args);
    sendNavigateEvent('pushState', from);
  };

  const originalReplaceState = history.replaceState;
  history.replaceState = function (...args) {
    const from = location.href;
    originalReplaceState.apply(this, args);
    sendNavigateEvent('replaceState', from);
  };
}
