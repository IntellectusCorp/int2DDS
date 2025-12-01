//! MultiTopic - Topic that aggregates data from multiple topics.
//!
//! `MultiTopic` is an advanced topic variant intended to allow a DataReader to receive data
//! from multiple related topics and potentially combine or transform the data through
//! a subscription expression.
//!
//! **Important**: This feature is currently **not supported**. (It is an optional DDS feature — planned for future implementation)
//!
//! Using this feature may lead to incomplete behavior or unexpected results at compile or runtime.
//!
// Currently not supported

#[allow(dead_code)]
#[derive(Clone)]
pub struct MultiTopic {
    topic_name: String,
    type_name: String,
    subscription_expression: String,
    expression_parameters: Vec<String>,
}

impl MultiTopic {
    #[allow(dead_code)]
    pub(crate) fn new(
        topic_name: &str,
        type_name: &str,
        subscription_expression: &str,
        expression_parameters: Vec<String>,
    ) -> Self {
        let topic_name = topic_name.to_owned();
        let type_name = type_name.to_owned();
        let subscription_expression = subscription_expression.to_owned();
        Self { topic_name, type_name, subscription_expression, expression_parameters }
    }
    pub fn get_expression_parameters() {
        // out: ReturnCode_t, expression_parameters: string[]
        todo!()
    }

    pub fn set_expression_parameters() {
        // in: expression_parameters: string[]
        // out: ReturnCode_t
        todo!()
    }
}
