pub mod mouse_listener;
pub mod text_selection;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    // 简单的模拟剪贴板管理器
    #[derive(Debug)]
    struct TestClipboardManager {
        content: Option<String>,
        history: Vec<String>,
    }

    impl TestClipboardManager {
        fn new() -> Self {
            Self {
                content: None,
                history: Vec::new(),
            }
        }

        fn set_content(&mut self, content: &str) {
            self.content = Some(content.to_string());
        }

        fn get_content(&self) -> Option<String> {
            self.content.clone()
        }

        fn add_to_history(&mut self, content: String) {
            self.history.push(content);
        }

        fn get_history(&self) -> Vec<String> {
            self.history.clone()
        }
    }

    #[test]
    fn test_basic_functionality() {
        let manager = Arc::new(Mutex::new(TestClipboardManager::new()));

        // 基本功能测试
        {
            let mut mgr = manager.lock().unwrap();
            mgr.set_content("test content");
            mgr.add_to_history("history item 1".to_string());
        }

        let content = {
            let mgr = manager.lock().unwrap();
            mgr.get_content()
        };

        assert_eq!(content, Some("test content".to_string()));
    }
}