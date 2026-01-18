pub fn escape_selector(selector: &str) -> String {
    selector.replace('\\', "\\\\").replace('\'', "\\'")
}

const ELEMENT_UTILS: &str = r#"
const INTERACTIVE_TAGS = new Set(['a','button','input','select','textarea','details','summary']);
const FORM_TAGS = new Set(['input','select','textarea','form','label','fieldset','legend','datalist','output','option','optgroup']);
const NAV_TAGS = new Set(['a','nav','menu','menuitem']);
const MEDIA_TAGS = new Set(['img','video','audio','picture','svg','canvas','iframe','embed','object']);
const TEXT_TAGS = new Set(['p','span','h1','h2','h3','h4','h5','h6','li','td','th','dt','dd','blockquote','pre','code','em','strong','label']);
const INTERACTIVE_ROLES = new Set(['button','link','checkbox','radio','textbox','combobox','listbox','menu','menubar','menuitem','menuitemcheckbox','menuitemradio','option','slider','spinbutton','switch','tab','treeitem','searchbox','gridcell','row']);
const NAV_ROLES = new Set(['link','navigation','menu','menubar','menuitem','tab','tablist']);

function isVisible(el) {
    if (!el || el.nodeType !== 1) return false;
    const style = window.getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden') return false;
    if (parseFloat(style.opacity || '1') === 0) return false;
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
}

function getRole(el) {
    const explicit = el.getAttribute('role');
    if (explicit) return explicit;
    const tag = el.tagName.toLowerCase();
    const type = el.getAttribute('type');
    const roleMap = {
        'a': el.hasAttribute('href') ? 'link' : null,
        'button': 'button',
        'input': type === 'checkbox' ? 'checkbox' : type === 'radio' ? 'radio' : type === 'range' ? 'slider' : type === 'submit' || type === 'button' ? 'button' : 'textbox',
        'select': 'combobox',
        'textarea': 'textbox',
        'img': 'img',
        'nav': 'navigation',
        'main': 'main',
        'header': 'banner',
        'footer': 'contentinfo',
        'aside': 'complementary',
        'article': 'article',
        'section': 'region',
        'form': 'form',
        'table': 'table',
        'ul': 'list',
        'ol': 'list',
        'li': 'listitem',
        'h1': 'heading', 'h2': 'heading', 'h3': 'heading', 'h4': 'heading', 'h5': 'heading', 'h6': 'heading'
    };
    return roleMap[tag] || null;
}

function getCategory(el) {
    const tag = el.tagName.toLowerCase();
    const role = getRole(el);
    if (FORM_TAGS.has(tag) && tag !== 'a') return 'form';
    if (role && INTERACTIVE_ROLES.has(role)) return 'interactive';
    if (INTERACTIVE_TAGS.has(tag)) return tag === 'a' ? 'navigation' : 'interactive';
    if (NAV_TAGS.has(tag) || (role && NAV_ROLES.has(role))) return 'navigation';
    if (MEDIA_TAGS.has(tag)) return 'media';
    if (TEXT_TAGS.has(tag)) return 'text';
    return 'container';
}

function genSelector(el) {
    if (el.id) return '#' + CSS.escape(el.id);
    const tag = el.tagName.toLowerCase();
    const ariaLabel = el.getAttribute('aria-label');
    if (ariaLabel) return `${tag}[aria-label="${CSS.escape(ariaLabel)}"]`;
    const name = el.getAttribute('name');
    if (name) return `${tag}[name="${CSS.escape(name)}"]`;
    const dataTestId = el.getAttribute('data-testid') || el.getAttribute('data-test-id');
    if (dataTestId) return `[data-testid="${CSS.escape(dataTestId)}"]`;
    const classes = Array.from(el.classList).filter(c => c && !c.match(/^(js-|is-|has-)/)).slice(0, 2);
    if (classes.length) {
        const sel = tag + '.' + classes.map(c => CSS.escape(c)).join('.');
        if (document.querySelectorAll(sel).length === 1) return sel;
    }
    let path = tag;
    let parent = el.parentElement;
    let depth = 0;
    while (parent && depth < 3) {
        const siblings = Array.from(parent.children).filter(c => c.tagName === el.tagName);
        if (siblings.length > 1) {
            const idx = siblings.indexOf(el) + 1;
            path = `${parent.tagName.toLowerCase()} > ${tag}:nth-of-type(${idx})`;
            if (document.querySelectorAll(path).length === 1) return path;
        }
        parent = parent.parentElement;
        depth++;
    }
    return tag;
}

function getLabel(el) {
    const ariaLabel = el.getAttribute('aria-label');
    if (ariaLabel) return ariaLabel.trim();
    const ariaLabelledBy = el.getAttribute('aria-labelledby');
    if (ariaLabelledBy) {
        const parts = ariaLabelledBy.split(/\s+/).map(id => document.getElementById(id)?.textContent?.trim()).filter(Boolean);
        if (parts.length) return parts.join(' ');
    }
    const title = el.getAttribute('title');
    if (title) return title.trim();
    const alt = el.getAttribute('alt');
    if (alt) return alt.trim();
    const placeholder = el.getAttribute('placeholder');
    if (placeholder) return placeholder.trim();
    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT') {
        const id = el.id;
        if (id) {
            const label = document.querySelector(`label[for="${id}"]`);
            if (label) return label.textContent?.trim() || null;
        }
    }
    return null;
}

function getText(el, maxLen) {
    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') return el.value || null;
    if (el.tagName === 'SELECT') return el.options[el.selectedIndex]?.text || null;
    const text = el.textContent?.trim();
    return text && text.length <= maxLen ? text : text?.substring(0, maxLen) || null;
}
"#;

pub fn visibility_check(selector: &str, check_visible: bool) -> String {
    let escaped = escape_selector(selector);
    let (condition, default_return) = if check_visible {
        (
            "style.display!=='none'&&style.visibility!=='hidden'&&parseFloat(style.opacity||'1')>0&&rect.width>0&&rect.height>0",
            "false",
        )
    } else {
        (
            "style.display==='none'||style.visibility==='hidden'||parseFloat(style.opacity||'1')===0||rect.width===0||rect.height===0",
            "true",
        )
    };

    format!(
        r#"(function(){{const el=document.querySelector('{}');if(!el)return {};const style=window.getComputedStyle(el);const rect=el.getBoundingClientRect();return {}}})()"#,
        escaped, default_return, condition
    )
}

pub fn click_element(selector: &str) -> String {
    let escaped = escape_selector(selector);
    format!(
        r#"(function(){{const el=document.querySelector('{}');if(!el)return{{found:false}};el.scrollIntoView({{block:'center',behavior:'instant'}});el.click();return{{found:true}}}})()"#,
        escaped
    )
}

pub fn fill_element(selector: &str, text: &str) -> String {
    let escaped = escape_selector(selector);
    let escaped_text = escape_selector(text);
    format!(
        r#"(function(){{const el=document.querySelector('{}');if(!el)return{{found:false}};el.scrollIntoView({{block:'center',behavior:'instant'}});el.focus();el.value='{}';el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}));return{{found:true}}}})()"#,
        escaped, escaped_text
    )
}

pub fn type_element(selector: &str, text: &str, delay_ms: u64) -> String {
    let escaped = escape_selector(selector);
    let escaped_text = escape_selector(text);
    format!(
        r#"(async function(){{const el=document.querySelector('{}');if(!el)return{{found:false}};el.scrollIntoView({{block:'center',behavior:'instant'}});el.focus();const text='{}';for(const c of text){{el.value+=c;el.dispatchEvent(new Event('input',{{bubbles:true}}));await new Promise(r=>setTimeout(r,{}))}}el.dispatchEvent(new Event('change',{{bubbles:true}}));return{{found:true}}}})()"#,
        escaped, escaped_text, delay_ms
    )
}

pub const MUTATION_OBSERVER: &str = r#"(function(){if(!window.__mutationCount){window.__mutationCount=0;const observer=new MutationObserver(()=>{window.__mutationCount++});observer.observe(document.body||document.documentElement,{childList:true,subtree:true,attributes:true})}return window.__mutationCount})()"#;

pub fn describe_visible_elements(
    selector: Option<&str>,
    filter_interactable: bool,
    filter_forms: bool,
    filter_navigation: bool,
    limit: usize,
    include_bounds: bool,
    include_selectors: bool,
) -> String {
    let root_selector = selector
        .map(|s| format!("document.querySelector('{}')", escape_selector(s)))
        .unwrap_or_else(|| "document.body".to_string());

    format!(
        r#"(function(){{{utils}

function getState(el) {{
    return {{
        disabled: el.disabled === true || el.getAttribute('aria-disabled') === 'true',
        checked: el.checked === true || el.getAttribute('aria-checked') === 'true',
        selected: el.selected === true || el.getAttribute('aria-selected') === 'true',
        expanded: el.getAttribute('aria-expanded') === 'true',
        readonly: el.readOnly === true || el.getAttribute('aria-readonly') === 'true',
        required: el.required === true || el.getAttribute('aria-required') === 'true'
    }};
}}

function hasState(state) {{
    return state.disabled || state.checked || state.selected || state.expanded || state.readonly || state.required;
}}

const root = {root};
if (!root) return null;

const filterInteractable = {filter_interactable};
const filterForms = {filter_forms};
const filterNavigation = {filter_navigation};
const limit = {limit};
const includeBounds = {include_bounds};
const includeSelectors = {include_selectors};

const elements = [];
let totalVisible = 0;
let interactive = 0;
let forms = 0;
let navigation = 0;

const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT, {{
    acceptNode: function(node) {{
        return isVisible(node) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
    }}
}});

let node = walker.currentNode;
while (node) {{
    if (node.nodeType === 1 && isVisible(node)) {{
        const category = getCategory(node);
        const isInteractive = category === 'interactive';
        const isForm = category === 'form';
        const isNav = category === 'navigation';

        if (isInteractive) interactive++;
        if (isForm) forms++;
        if (isNav) navigation++;

        const shouldInclude =
            (!filterInteractable && !filterForms && !filterNavigation) ||
            (filterInteractable && isInteractive) ||
            (filterForms && isForm) ||
            (filterNavigation && isNav);

        if (shouldInclude && (category !== 'container' || node === root)) {{
            totalVisible++;
            if (elements.length < limit) {{
                const state = getState(node);
                const el = {{
                    index: elements.length,
                    tag: node.tagName.toLowerCase(),
                    role: getRole(node),
                    label: getLabel(node),
                    text: getText(node, 200),
                    category: category,
                    state: hasState(state) ? state : null,
                    selector: includeSelectors ? genSelector(node) : null,
                    bounds: null
                }};
                if (includeBounds) {{
                    const rect = node.getBoundingClientRect();
                    el.bounds = {{
                        x: Math.round(rect.x),
                        y: Math.round(rect.y),
                        width: Math.round(rect.width),
                        height: Math.round(rect.height),
                        inViewport: rect.top >= 0 && rect.left >= 0 && rect.bottom <= window.innerHeight && rect.right <= window.innerWidth
                    }};
                }}
                elements.push(el);
            }}
        }}
    }}
    node = walker.nextNode();
}}

return {{
    page: {{
        url: window.location.href,
        title: document.title,
        viewport: {{ width: window.innerWidth, height: window.innerHeight }}
    }},
    elements: elements,
    summary: {{
        totalVisible: totalVisible,
        interactive: interactive,
        forms: forms,
        navigation: navigation,
        truncated: totalVisible > limit
    }}
}};
}})()"#,
        utils = ELEMENT_UTILS,
        root = root_selector,
        filter_interactable = filter_interactable,
        filter_forms = filter_forms,
        filter_navigation = filter_navigation,
        limit = limit,
        include_bounds = include_bounds,
        include_selectors = include_selectors
    )
}

pub fn label_elements(selector: Option<&str>) -> String {
    let root_selector = selector
        .map(|s| format!("document.querySelector('{}')", escape_selector(s)))
        .unwrap_or_else(|| "document.body".to_string());

    format!(
        r#"(function(){{{utils}
const INTERACTIVE_SELECTORS = 'a[href],button,input,select,textarea,[role="button"],[role="link"],[role="checkbox"],[role="radio"],[role="textbox"],[role="combobox"],[role="listbox"],[role="menuitem"],[role="tab"],[tabindex]:not([tabindex="-1"])';

const root = {root};
if (!root) return null;

const elements = Array.from(root.querySelectorAll(INTERACTIVE_SELECTORS)).filter(isVisible);
const labels = [];

window.__labelOverlays = window.__labelOverlays || [];

elements.forEach((el, i) => {{
    const rect = el.getBoundingClientRect();

    const overlay = document.createElement('div');
    overlay.className = '__cdtcli_label';
    overlay.textContent = i;
    overlay.style.cssText = `
        position: fixed;
        left: ${{rect.left - 2}}px;
        top: ${{rect.top - 2}}px;
        min-width: 18px;
        height: 18px;
        background: #e53935;
        color: white;
        font-size: 11px;
        font-weight: bold;
        font-family: Arial, sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
        border-radius: 9px;
        z-index: 2147483647;
        pointer-events: none;
        padding: 0 4px;
        box-shadow: 0 1px 3px rgba(0,0,0,0.3);
    `;
    document.body.appendChild(overlay);
    window.__labelOverlays.push(overlay);

    labels.push({{
        id: i,
        selector: genSelector(el),
        tag: el.tagName.toLowerCase(),
        role: getRole(el),
        label: getLabel(el),
        text: getText(el, 50),
        bounds: {{
            x: Math.round(rect.x),
            y: Math.round(rect.y),
            width: Math.round(rect.width),
            height: Math.round(rect.height)
        }}
    }});
}});

return {{ labels }};
}})()"#,
        utils = ELEMENT_UTILS,
        root = root_selector
    )
}

pub fn remove_labels() -> &'static str {
    r#"(function(){
if (window.__labelOverlays) {
    window.__labelOverlays.forEach(el => el.remove());
    window.__labelOverlays = [];
}
})()"#
}

pub fn resolve_ref(ref_id: &str) -> String {
    let escaped = escape_selector(ref_id);
    format!(
        r#"(function(){{{utils}
const REF_CATEGORIES = {{'i':'interactive','f':'form','n':'navigation','m':'media','t':'text','c':'container'}};

const ref = '{ref_id}';
const match = ref.match(/^([ifnmtc])(\d+)$/);
if (!match) return null;

const [, prefix, indexStr] = match;
const targetCategory = REF_CATEGORIES[prefix];
const targetIndex = parseInt(indexStr, 10);

const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT, {{
    acceptNode: function(node) {{
        return isVisible(node) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
    }}
}});

let index = 0;
let node = walker.currentNode;
while (node) {{
    if (node.nodeType === 1 && isVisible(node)) {{
        const category = getCategory(node);
        if (category !== 'container' && category === targetCategory) {{
            if (index === targetIndex) {{
                return {{ selector: genSelector(node), tag: node.tagName.toLowerCase(), found: true }};
            }}
            index++;
        }}
    }}
    node = walker.nextNode();
}}

return {{ found: false, error: `Element with ref '${{ref}}' not found` }};
}})()"#,
        utils = ELEMENT_UTILS,
        ref_id = escaped
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_selector() {
        assert_eq!(escape_selector("div"), "div");
        assert_eq!(escape_selector("div's"), "div\\'s");
        assert_eq!(escape_selector("div\\class"), "div\\\\class");
    }

    #[test]
    fn test_visibility_check_visible() {
        let script = visibility_check("#test", true);
        assert!(script.contains("querySelector('#test')"));
        assert!(script.contains("return false")); // default return
    }

    #[test]
    fn test_visibility_check_hidden() {
        let script = visibility_check("#test", false);
        assert!(script.contains("querySelector('#test')"));
        assert!(script.contains("return true")); // default return
    }

    #[test]
    fn test_click_element() {
        let script = click_element("#btn");
        assert!(script.contains("querySelector('#btn')"));
        assert!(script.contains("click()"));
    }

    #[test]
    fn test_fill_element() {
        let script = fill_element("#input", "hello");
        assert!(script.contains("querySelector('#input')"));
        assert!(script.contains("value='hello'"));
    }

    #[test]
    fn test_type_element() {
        let script = type_element("#input", "hi", 50);
        assert!(script.contains("querySelector('#input')"));
        assert!(script.contains("setTimeout(r,50)"));
    }
}
