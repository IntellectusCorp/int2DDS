//! Topic name synthesis rules (7.4.1, 7.4.2)
//!
//! BNF: <topic_name> ::= <interface_name> "_" <service_name> "_" [ "Request" | "Reply" ]
//!                      | <user_def_alpha_num>
//!
//! For request-reply style, interface_name is NOT automatically included (7.4.1).
//! Priority: runtime params > annotation > default (7.4.2.3)

const DEFAULT_SERVICE_NAME: &str = "Service";

pub(crate) struct TopicNameConfig {
    pub(crate) interface_name: Option<String>,
    pub(crate) service_name: Option<String>,
    pub(crate) request_topic_override: Option<String>,
    pub(crate) reply_topic_override: Option<String>,
    pub(crate) request_type_override: Option<String>,
    pub(crate) reply_type_override: Option<String>,
}

impl TopicNameConfig {
    pub(crate) fn request_topic(&self) -> String {
        if let Some(ref name) = self.request_topic_override {
            return name.clone();
        }
        self.synthesize("Request")
    }

    pub(crate) fn reply_topic(&self) -> String {
        if let Some(ref name) = self.reply_topic_override {
            return name.clone();
        }
        self.synthesize("Reply")
    }

    /// Synthesize the request type name (7.5.1.1.6)
    /// "${interfaceName}_Request"
    pub(crate) fn request_type(&self) -> Option<String> {
        self.request_type_override
            .clone()
            .or_else(|| self.interface_name.as_ref().map(|iface| format!("{}_Request", iface)))
    }

    /// Synthesize the reply type name (7.5.1.1.7)
    /// "${interfaceName}_Reply"
    pub(crate) fn reply_type(&self) -> Option<String> {
        self.reply_type_override
            .clone()
            .or_else(|| self.interface_name.as_ref().map(|iface| format!("{}_Reply", iface)))
    }

    fn synthesize(&self, suffix: &str) -> String {
        let service = self.service_name.as_deref().unwrap_or(DEFAULT_SERVICE_NAME);

        match &self.interface_name {
            Some(iface) => format!("{}_{}_{}", fully_qualified_name(iface), service, suffix),
            None => format!("{}_{}", service, suffix),
        }
    }
}

// Converts module-separated name (e.g. "robot::control::RobotControl")
// to underscore-separated (e.g. "robot_control_RobotControl")
fn fully_qualified_name(name: &str) -> String {
    name.replace("::", "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_function_call_style() {
        let config = TopicNameConfig {
            interface_name: Some("robot::RobotControl".into()),
            service_name: None,
            request_topic_override: None,
            reply_topic_override: None,
            request_type_override: None,
            reply_type_override: None,
        };
        assert_eq!(config.request_topic(), "robot_RobotControl_Service_Request");
        assert_eq!(config.reply_topic(), "robot_RobotControl_Service_Reply");
    }

    #[test]
    fn request_reply_style_no_interface() {
        let config = TopicNameConfig {
            interface_name: None,
            service_name: Some("MyService".into()),
            request_topic_override: None,
            reply_topic_override: None,
            request_type_override: None,
            reply_type_override: None,
        };
        assert_eq!(config.request_topic(), "MyService_Request");
        assert_eq!(config.reply_topic(), "MyService_Reply");
    }

    #[test]
    fn custom_service_name() {
        let config = TopicNameConfig {
            interface_name: Some("RobotControl".into()),
            service_name: Some("MyRobot".into()),
            request_topic_override: None,
            reply_topic_override: None,
            request_type_override: None,
            reply_type_override: None,
        };
        assert_eq!(config.request_topic(), "RobotControl_MyRobot_Request");
    }

    #[test]
    fn annotation_override() {
        let config = TopicNameConfig {
            interface_name: Some("RobotControl".into()),
            service_name: None,
            request_topic_override: Some("RobotRequestTopic".into()),
            reply_topic_override: Some("RobotReplyTopic".into()),
            request_type_override: None,
            reply_type_override: None,
        };
        assert_eq!(config.request_topic(), "RobotRequestTopic");
        assert_eq!(config.reply_topic(), "RobotReplyTopic");
    }
}
