    #[derive(Clone)]
    struct MockProvider {
        label: String,
        fail_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        max_failures: u32,
    }

    impl MockProvider {
        fn new(label: &str, max_failures: u32) -> Self {
            Self {
                label: label.to_string(),
                fail_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                max_failures,
            }
        }

        fn fail_n_then_succeed(label: &str, n: u32) -> Self {
            Self {
                label: label.to_string(),
                fail_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                max_failures: n,
            }
        }

        fn always_fail(label: &str) -> Self {
            Self {
                label: label.to_string(),
                fail_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                max_failures: u32::MAX,
            }
        }
    }

    impl InferenceProvider for MockProvider {
        fn stream_inference<'a>(
            &'a self,
            _request: &'a MessageRequest,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<StreamEvent>, ApiError>> + Send + 'a>> {
            let label = self.label.clone();
            let fail_count = self.fail_count.clone();
            let max_failures = self.max_failures;

            Box::pin(async move {
                let current = fail_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if current < max_failures {
                    return Err(ApiError::ProviderRefusal(format!(
                        "provider {} failing purposefully ({} < {})",
                        label, current, max_failures
                    )));
                }

                Ok(vec![
                    StreamEvent::ContentBlockDelta(crate::types::ContentBlockDeltaEvent {
                        index: 0,
                        delta: crate::types::ContentBlockDelta::TextDelta {
                            text: format!("Success from {}", label),
                        },
                    }),
                    StreamEvent::MessageStop(crate::types::MessageStopEvent {
                        stop_reason: "end_turn".to_string(),
                    }),
                ])
            })
        }

        fn provider_label(&self) -> &str {
            &self.label
        }
    }

    #[derive(Clone)]
    struct MockRefusingProvider {
        label: String,
        refusal_message: String,
    }

    impl InferenceProvider for MockRefusingProvider {
        fn stream_inference<'a>(
            &'a self,
            _request: &'a MessageRequest,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<StreamEvent>, ApiError>> + Send + 'a>>
        {
            let refusal = self.refusal_message.clone();
            Box::pin(async move { Err(ApiError::ProviderRefusal(refusal)) })
        }

        fn provider_label(&self) -> &str {
            &self.label
        }
    }
