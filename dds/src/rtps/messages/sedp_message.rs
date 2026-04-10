use std::sync::Arc;

use crate::{
    common::builtin::topic::{
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    },
    rtps::{
        builtin::data::discovered_data::{DiscoveredReaderData, DiscoveredWriterData},
        common::{
            entity_id::EntityId,
            parameters::{ParameterId, ParameterValue},
            sequence::SequenceNumber,
            types::{SerializedData, SubmessagePayload},
        },
        messages::{
            header::Header,
            rtps_messages::RtpsMessage,
            submessage::Submessage,
            submessage_body::SubmessageBody,
            submessage_header::SubmessageHeader,
            submessage_header_flag::{SubmessageFlagType, SubmessageHeaderFlag},
            submessage_id::SubmessageId,
            submessages::data::Data,
        },
    },
    serialize::pl_cdr::PlCdrParser,
};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct SEDPMessage<T> {
    data: T,
    rtps_message: Arc<RtpsMessage<'static>>,
}

#[allow(dead_code)]
impl SEDPMessage<DiscoveredWriterData> {
    pub(crate) fn new(writer_data: DiscoveredWriterData) -> Self {
        // Get GuidPrefix from PublicationBuiltinTopicData's Guid and create RTPS header
        let writer_guid = writer_data.publication_builtin_topic_data.endpoint_guid();
        let mut rtps_message = RtpsMessage::new(Header::new(writer_guid.prefix()));

        rtps_message.add_submessage(Self::create_publication_data_submessage(&writer_data));

        Self { data: writer_data, rtps_message: Arc::new(rtps_message) }
    }

    pub(crate) fn from_serialized_payload(
        payload: &[u8],
        is_big_endian: bool,
    ) -> Result<DiscoveredWriterData, String> {
        // Use PlCdrParser to parse all parameters for consistency
        let parser = PlCdrParser::new(is_big_endian);
        let _parameters = parser.parse(payload)?;

        // Parse PublicationBuiltinTopicData from payload
        let publication_builtin_topic_data =
            PublicationBuiltinTopicData::from_serialized_data(payload)?;

        Ok(DiscoveredWriterData { publication_builtin_topic_data })
    }

    fn create_publication_data_submessage(
        writer_data: &DiscoveredWriterData,
    ) -> Submessage<'static> {
        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::DATA);
        data_header_flag.add_flag(SubmessageFlagType::DataFlag, SubmessageId::DATA);

        let mut data = Data::new(
            EntityId::SEDP_BUILTIN_PUBLICATIONS_READER,
            EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
            SequenceNumber::new(0, 1), // Temporary sequence number
        );

        data.add_serialized_data(SubmessagePayload::Owned(
            Self::create_publication_serialized_data(writer_data),
        ));
        let length = data.octets_to_next_header();

        let submessage_body = SubmessageBody::Data(data);

        Submessage {
            header: SubmessageHeader::new(SubmessageId::DATA, data_header_flag.flag, length),
            body: submessage_body,
        }
    }

    pub(crate) fn create_publication_serialized_data(
        writer_data: &DiscoveredWriterData,
    ) -> SerializedData {
        // Serialize PublicationBuiltinTopicData
        let base_payload = writer_data.publication_builtin_topic_data.to_serialized_data();

        SerializedData::from(base_payload.to_vec())
    }
}

#[allow(dead_code)]
impl SEDPMessage<DiscoveredReaderData> {
    pub(crate) fn new(reader_data: DiscoveredReaderData) -> Self {
        // Get GuidPrefix from SubscriptionBuiltinTopicData's Guid and create RTPS header
        let reader_guid = reader_data.subscription_builtin_topic_data.endpoint_guid();
        let mut rtps_message = RtpsMessage::new(Header::new(reader_guid.prefix()));

        rtps_message.add_submessage(Self::create_subscription_data_submessage(&reader_data));

        Self { data: reader_data, rtps_message: Arc::new(rtps_message) }
    }

    pub(crate) fn from_serialized_payload(
        payload: &[u8],
        is_big_endian: bool,
    ) -> Result<DiscoveredReaderData, String> {
        // Use PlCdrParser to parse all parameters at once
        let parser = PlCdrParser::new(is_big_endian);
        let parameters = parser.parse(payload)?;

        // Parse SubscriptionBuiltinTopicData from payload
        let subscription_builtin_topic_data =
            SubscriptionBuiltinTopicData::from_serialized_data(payload)?;

        // Find ContentFilterProperty from parsed parameters
        let content_filter = parameters.iter().find_map(|param| {
            if param.id == ParameterId::PidContentFilterProperty {
                // info!("SEDPMessage: Found PidContentFilterProperty parameter!");
                if let ParameterValue::ContentFilterProperty(property) = &param.value {
                    // debug!("SEDPMessage: Successfully extracted ContentFilterProperty value");
                    return Some(property.clone());
                } else {
                    // debug!(
                    //     "SEDPMessage: PidContentFilterProperty found but value type mismatch: {:?}",
                    //     param.value
                    // );
                }
            }
            None
        });

        Ok(DiscoveredReaderData { subscription_builtin_topic_data, content_filter })
    }

    fn create_subscription_data_submessage(
        reader_data: &DiscoveredReaderData,
    ) -> Submessage<'static> {
        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::DATA);
        data_header_flag.add_flag(SubmessageFlagType::DataFlag, SubmessageId::DATA);

        let mut data = Data::new(
            EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER,
            EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER,
            SequenceNumber::new(0, 1), // Temporary sequence number
        );

        data.add_serialized_data(SubmessagePayload::Owned(
            Self::create_subscription_serialized_data(reader_data),
        ));
        let length = data.octets_to_next_header();

        let submessage_body = SubmessageBody::Data(data);

        Submessage {
            header: SubmessageHeader::new(SubmessageId::DATA, data_header_flag.flag, length),
            body: submessage_body,
        }
    }

    pub(crate) fn create_subscription_serialized_data(
        reader_data: &DiscoveredReaderData,
    ) -> SerializedData {
        // Serialize SubscriptionBuiltinTopicData
        let base_payload = reader_data.subscription_builtin_topic_data.to_serialized_data();

        // Serialize ContentFilterProperty
        let filter_payload = reader_data
            .content_filter
            .as_ref()
            .and_then(|filter| filter.to_serialized_payload(true).ok());

        // Merge the two data to create the final payload
        let payload = crate::serialize::pl_cdr::merge_serialized_data(
            base_payload.to_vec(),
            filter_payload,
            true, // little endian
        );

        SerializedData::from(payload)
    }
}

#[allow(dead_code)]
impl<T> SEDPMessage<T> {
    pub(crate) fn rtps_message(self) -> Arc<RtpsMessage<'static>> {
        self.rtps_message.clone()
    }
}
