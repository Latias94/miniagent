use siumai::types::{ChatMessage, ContentPart, MessageContent};

pub trait TokenEstimator: Send + Sync {
    fn count_messages(&self, messages: &[ChatMessage]) -> usize;
}

pub struct ApproxEstimator;

impl ApproxEstimator {
    fn count_text(s: &str) -> usize {
        (s.chars().count() as f64 / 2.5) as usize
    }
}

impl TokenEstimator for ApproxEstimator {
    fn count_messages(&self, messages: &[ChatMessage]) -> usize {
        let mut total = 0usize;
        for m in messages {
            match &m.content {
                MessageContent::Text(t) => {
                    total += Self::count_text(t);
                }
                MessageContent::MultiModal(parts) => {
                    for p in parts {
                        match p {
                            ContentPart::Text { text } => total += Self::count_text(text),
                            ContentPart::ToolCall { arguments, .. } => {
                                let s = serde_json::to_string(arguments).unwrap_or_default();
                                total += Self::count_text(&s);
                            }
                            ContentPart::ToolResult { output, .. } => {
                                total += Self::count_text(&output.to_string_lossy());
                            }
                            _ => {}
                        }
                    }
                }
            }
            total += 4; // overhead per message
        }
        total
    }
}

#[cfg(feature = "tiktoken")]
pub struct TiktokenEstimator {
    bpe: tiktoken_rs::CoreBPE,
}

#[cfg(feature = "tiktoken")]
impl TiktokenEstimator {
    pub fn cl100k() -> Self {
        Self {
            bpe: tiktoken_rs::cl100k_base().expect("cl100k_base"),
        }
    }
    fn count_str(&self, s: &str) -> usize {
        self.bpe.encode_ordinary(s).len()
    }
}

#[cfg(feature = "tiktoken")]
impl TokenEstimator for TiktokenEstimator {
    fn count_messages(&self, messages: &[ChatMessage]) -> usize {
        let mut total = 0usize;
        for m in messages {
            match &m.content {
                MessageContent::Text(t) => {
                    total += self.count_str(t);
                }
                MessageContent::MultiModal(parts) => {
                    for p in parts {
                        match p {
                            ContentPart::Text { text } => total += self.count_str(text),
                            ContentPart::ToolCall { arguments, .. } => {
                                let s = serde_json::to_string(arguments).unwrap_or_default();
                                total += self.count_str(&s);
                            }
                            ContentPart::ToolResult { output, .. } => {
                                total += self.count_str(&output.to_string_lossy());
                            }
                            _ => {}
                        }
                    }
                }
            }
            total += 4;
        }
        total
    }
}
