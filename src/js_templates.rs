pub fn escape_selector(selector: &str) -> String {
    selector.replace('\\', "\\\\").replace('\'', "\\'")
}

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
