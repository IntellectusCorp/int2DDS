//! ContentFilteredTopic - Topic variant that filters data based on content.
//!
//! A `ContentFilteredTopic` is a specialization of `Topic` that allows DataReaders to
//! receive only data samples that match a specified filter expression. The filtering
//! is performed on the subscriber side, reducing unnecessary data processing in the
//! application when only a subset of published data is relevant.
//!
//! Content filtering uses SQL-like expressions to specify which samples should be delivered
//! based on their field values.
//!
//! # Filter Expressions
//!
//! Filter expressions use SQL-92 syntax to specify conditions:
//! - Comparison operators: `=`, `<>`, `<`, `>`, `<=`, `>=`
//! - Logical operators: `AND`, `OR`, `NOT`
//! - Field access: Direct reference to struct fields
//! - Parameters: Placeholder values using `%0`, `%1`, etc.

use std::{
    any::Any,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
};

use crate::{
    common::instance_handle::InstanceHandle,
    core::error::{DdsError, DdsResult},
    domain::domain_participant::DomainParticipant,
    rtps::common::guid::Guid,
    topic::sql::{ast::Expression, parse_expression},
};

use super::{
    topic::Topic,
    topic_description::{
        impl_topic_description, impl_topic_description_impl, TopicDescription,
        TopicDescriptionInternal,
    },
};

#[derive(Clone)]
pub struct ContentFilteredTopic {
    guid: Guid,
    pub(crate) self_ref: Option<Arc<ContentFilteredTopic>>,
    deleted: Arc<AtomicBool>,
    related_topic: Option<Weak<Topic>>,
    topic_name: String,
    type_name: String,
    participant: Option<Weak<DomainParticipant>>,
    filter_expression: String,
    expression_parameters: Arc<Mutex<Vec<String>>>,
    pub(crate) parsed_expression: Expression,
}
impl PartialEq for ContentFilteredTopic {
    fn eq(&self, other: &Self) -> bool {
        self.guid() == other.guid()
    }
}
impl Eq for ContentFilteredTopic {}
impl Drop for ContentFilteredTopic {
    fn drop(&mut self) {
        // Only handle drop for the last reference (not clones)
        if let Some(ref self_arc) = self.self_ref {
            if Arc::strong_count(self_arc) > 1 {
                return; // This is a clone, skip orphan handling
            }
        } else {
            return; // Never fully initialized
        }

        if !self.deleted.load(Ordering::SeqCst) {
            if let Ok(ref participant) = self.get_participant() {
                if let Ok(topic) = self.get_related_topic() {
                    if let Ok(topic_handle) = topic.get_instance_handle() {
                        let cft_handle = InstanceHandle::from_guid(&self.guid);
                        participant.handle_contentfilteredtopic_drop(&topic_handle, &cft_handle);
                    }
                }
            }
        }
    }
}

impl_topic_description!(ContentFilteredTopic);

impl ContentFilteredTopic {
    pub(crate) fn new(
        topic_name: &str,
        related_topic: &Arc<Topic>,
        filter_expression: &str,
        expression_parameters: Vec<String>,
        handle: InstanceHandle,
        participant: &Arc<DomainParticipant>,
    ) -> DdsResult<Self> {
        let expression = parse_expression(filter_expression, false)?;
        expression.validate_expression_parameters(&expression_parameters)?;

        let filter_expression = filter_expression.to_owned();
        let mut cft = Self {
            guid: handle.to_guid(),
            self_ref: None,
            deleted: Arc::new(AtomicBool::new(false)),
            related_topic: Some(Arc::downgrade(related_topic)),
            topic_name: topic_name.to_owned(),
            type_name: related_topic.get_type_name().to_string(),
            participant: Some(Arc::downgrade(participant)),
            filter_expression,
            expression_parameters: Arc::new(Mutex::new(expression_parameters)),
            parsed_expression: expression,
        };
        let cft_arc = Arc::new(cft.clone());
        cft.self_ref = Some(cft_arc); // Without Arc, the new() function completes and memory is freed. The StatusCondition's entity field returns None.
        Ok(cft)
    }
    pub fn get_filter_expression(&self) -> DdsResult<String> {
        self.is_deleted()?;
        Ok(self.filter_expression.clone())
    }

    pub fn get_related_topic(&self) -> DdsResult<Topic> {
        self.is_deleted()?;
        if let Some(weak_ref) = self.related_topic.as_ref() {
            // Attempt to upgrade Weak<T> to Arc<T>
            if let Some(related_topic_arc) = weak_ref.upgrade() {
                return Ok((*related_topic_arc).clone());
            }
        }

        // If related_topic is None or the reference has expired
        Err(DdsError::Error("Related Topic reference is invalid or expired".to_string()))
    }

    pub fn get_expression_parameters(&self) -> DdsResult<Vec<String>> {
        self.is_deleted()?;
        match self.expression_parameters.lock() {
            Ok(expression_parameters) => Ok(expression_parameters.clone()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub fn set_expression_parameters(&self, expression_parameters: Vec<String>) -> DdsResult<()> {
        self.is_deleted()?;
        self.parsed_expression.validate_expression_parameters(&expression_parameters)?;

        match self.expression_parameters.lock() {
            Ok(mut prev_params) => {
                *prev_params = expression_parameters;
                Ok(())
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub(crate) fn guid(&self) -> Guid {
        self.guid
    }

    pub(crate) fn delete(&mut self) {
        self.self_ref = None;
        self.deleted.store(true, Ordering::SeqCst);
    }

    fn is_deleted(&self) -> DdsResult<()> {
        if self.deleted.load(Ordering::SeqCst) {
            Err(DdsError::AlreadyDeleted)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {


    use super::*;
    use crate::{
        common::instance_handle::InstanceHandle,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::status::StatusMask,
        rtps::common::entity_id::EntityId,
        topic::qos::TopicQos,
        DdsType,
    };
    use std::sync::Arc;

    #[derive(DdsType)]
    struct TestData {
        pub id: i32,
        pub name: String,
        pub score: f64,
        pub grade: char,
        pub active: bool,
    }

    #[test]
    fn test_valid_parameter_validation() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic = domain_participant
            .create_topic::<TestData>(
                "test_topic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let topic = Arc::new(topic);
        let handle = InstanceHandle::from_guid(&Guid::new(
            Guid::generate_unique_guid_prefix(),
            EntityId::UNKNOWN,
        ));

        // Valid: %0 parameter with 1 parameter provided
        let result = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "height > %0",
            vec!["100".to_string()],
            handle,
            &Arc::new(domain_participant.clone()),
        );
        assert!(result.is_ok());

        // Valid: %0 and %1 parameters with 2 parameters provided
        let result = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "height > %0 AND weight < %1",
            vec!["100".to_string(), "200".to_string()],
            handle,
            &Arc::new(domain_participant.clone()),
        );
        assert!(result.is_ok());

        // Valid: no parameters in expression
        let result = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "height > 100",
            vec![],
            handle,
            &Arc::new(domain_participant.clone()),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_parameter_validation() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic = domain_participant
            .create_topic::<TestData>(
                "test_topic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let topic = Arc::new(topic);
        let handle = InstanceHandle::from_guid(&Guid::new(
            Guid::generate_unique_guid_prefix(),
            EntityId::UNKNOWN,
        ));

        // Invalid: %1 parameter but only 1 parameter provided (index out of bounds)
        let result = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "height > %1",
            vec!["100".to_string()],
            handle,
            &Arc::new(domain_participant.clone()),
        );
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Parameter %1 not found"));
        }

        // Invalid: %0 parameter but no parameters provided
        let result = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "height > %0",
            vec![],
            handle,
            &Arc::new(domain_participant.clone()),
        );
        assert!(result.is_err());

        // Invalid: malformed expression
        let result = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "height >",
            vec!["100".to_string()],
            handle,
            &Arc::new(domain_participant.clone()),
        );
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Expected parameter"));
        }
    }

    #[test]
    fn test_set_expression_parameters_validation() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic = domain_participant
            .create_topic::<TestData>(
                "test_topic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let topic = Arc::new(topic);
        let handle = InstanceHandle::from_guid(&Guid::new(
            Guid::generate_unique_guid_prefix(),
            EntityId::UNKNOWN,
        ));

        // Create valid ContentFilteredTopic
        let cft = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "height > %0 AND weight < %1",
            vec!["100".to_string(), "200".to_string()],
            handle,
            &Arc::new(domain_participant.clone()),
        )
        .unwrap();

        // Valid: update with correct number of parameters
        let result = cft.set_expression_parameters(vec!["150".to_string(), "250".to_string()]);
        assert!(result.is_ok());

        // Invalid: update with wrong number of parameters
        let result = cft.set_expression_parameters(vec!["150".to_string()]);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Parameter %1 not found"));
        }

        // Invalid: update with too many parameters (should still work, extra parameters ignored)
        let result = cft.set_expression_parameters(vec![
            "150".to_string(),
            "250".to_string(),
            "300".to_string(),
        ]);
        assert!(result.is_ok()); // This should be valid - extra parameters are allowed
    }

    #[test]
    fn test_complex_expression_validation() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic = domain_participant
            .create_topic::<TestData>(
                "test_topic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let topic = Arc::new(topic);
        let handle = InstanceHandle::from_guid(&Guid::new(
            Guid::generate_unique_guid_prefix(),
            EntityId::UNKNOWN,
        ));

        // Complex expression with multiple parameters
        let result = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "(height > %0 AND weight < %1) OR (speed BETWEEN %2 AND %3)",
            vec!["100".to_string(), "200".to_string(), "50".to_string(), "100".to_string()],
            handle,
            &Arc::new(domain_participant.clone()),
        );
        assert!(result.is_ok());

        // Same expression with missing parameter
        let result = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "(height > %0 AND weight < %1) OR (speed BETWEEN %2 AND %3)",
            vec!["100".to_string(), "200".to_string(), "50".to_string()], // Missing %3
            handle,
            &Arc::new(domain_participant.clone()),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_between_predicate_validation() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic = domain_participant
            .create_topic::<TestData>(
                "test_topic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let topic = Arc::new(topic);
        let handle = InstanceHandle::from_guid(&Guid::new(
            Guid::generate_unique_guid_prefix(),
            EntityId::UNKNOWN,
        ));

        // Valid BETWEEN with parameters
        let result = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "score BETWEEN %0 AND %1",
            vec!["80".to_string(), "90".to_string()],
            handle,
            &Arc::new(domain_participant.clone()),
        );
        assert!(result.is_ok());

        // Valid NOT BETWEEN with parameters
        let result = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "score NOT BETWEEN %0 AND %1",
            vec!["80".to_string(), "90".to_string()],
            handle,
            &Arc::new(domain_participant.clone()),
        );
        assert!(result.is_ok());

        // Invalid: missing end parameter
        let result = ContentFilteredTopic::new(
            "content_test",
            &topic,
            "score BETWEEN %0 AND %1",
            vec!["80".to_string()], // Missing %1
            handle,
            &Arc::new(domain_participant.clone()),
        );
        assert!(result.is_err());
    }
}
