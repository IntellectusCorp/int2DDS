use std::convert::TryInto;

use smallvec::SmallVec;

use crate::{
    common::instance_handle::InstanceHandle,
    infrastructure::qos_policy::{DataRepresentationId, DataRepresentationQosPolicy},
    rtps::common::parameters::{Parameter, ParameterId, ParameterList, StatusInfo},
};

fn make_parameter<V>(id: ParameterId, value: V) -> Parameter
where
    V: Into<SmallVec<[u8; 16]>>,
{
    Parameter::new(id, value)
}

pub trait InlineQosParameters {
    fn get_key_hash(&self) -> Option<InstanceHandle>;
    fn set_key_hash(&mut self, key_hash: InstanceHandle);

    fn get_status_info(&self) -> Option<StatusInfo>;
    fn set_status_info(&mut self, status_info: StatusInfo);

    fn get_expects_inline_qos(&self) -> Option<bool>;
    fn set_expects_inline_qos(&mut self, expects: bool);

    fn get_group_seq_num(&self) -> Option<u64>;
    fn set_group_seq_num(&mut self, seq_num: u64);

    fn get_content_filter_info(&self) -> Option<Vec<u8>>;
    fn set_content_filter_info(&mut self, info: Vec<u8>);

    fn get_coherent_set(&self) -> Option<Vec<u8>>;
    fn set_coherent_set(&mut self, coherent_set: Vec<u8>);

    fn get_directed_write(&self) -> Option<Vec<u8>>;
    fn set_directed_write(&mut self, directed_write: Vec<u8>);

    fn get_original_writer_info(&self) -> Option<Vec<u8>>;
    fn set_original_writer_info(&mut self, info: Vec<u8>);

    fn get_group_coherent_set(&self) -> Option<Vec<u8>>;
    fn set_group_coherent_set(&mut self, coherent_set: Vec<u8>);

    fn get_writer_group_info(&self) -> Option<Vec<u8>>;
    fn set_writer_group_info(&mut self, info: Vec<u8>);

    fn get_secure_writer_group_info(&self) -> Option<Vec<u8>>;
    fn set_secure_writer_group_info(&mut self, info: Vec<u8>);

    fn get_data_representation(&self) -> Option<DataRepresentationQosPolicy>;
    fn set_data_representation(&mut self, data_representation: DataRepresentationQosPolicy);

    fn is_inline_qos_empty(&self) -> bool {
        self.get_key_hash().is_none()
            && self.get_status_info().is_none()
            && self.get_expects_inline_qos().is_none()
            && self.get_content_filter_info().is_none()
            && self.get_coherent_set().is_none()
            && self.get_directed_write().is_none()
            && self.get_original_writer_info().is_none()
            && self.get_group_coherent_set().is_none()
            && self.get_group_seq_num().is_none()
            && self.get_writer_group_info().is_none()
            && self.get_secure_writer_group_info().is_none()
            && self.get_data_representation().is_none()
    }
}

impl InlineQosParameters for ParameterList {
    fn get_key_hash(&self) -> Option<InstanceHandle> {
        self.iter()
            .find(|param| {
                param.parameter_id() == ParameterId::PidKeyHash && param.value().len() == 16
            })
            .and_then(|param| param.value().try_into().ok())
            .map(InstanceHandle::new)
    }

    fn set_key_hash(&mut self, key_hash: InstanceHandle) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidKeyHash);
        self.add_parameter(make_parameter(
            ParameterId::PidKeyHash,
            SmallVec::from_slice(key_hash.value()),
        ));
    }

    fn get_status_info(&self) -> Option<StatusInfo> {
        self.iter()
            .find(|param| {
                param.parameter_id() == ParameterId::PidStatusInfo && param.value().len() == 4
            })
            .and_then(|param| param.value().try_into().ok())
            .map(u32::from_be_bytes)
            .map(StatusInfo::new)
    }

    fn set_status_info(&mut self, status_info: StatusInfo) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidStatusInfo);
        self.add_parameter(make_parameter(
            ParameterId::PidStatusInfo,
            SmallVec::from_slice(&status_info.to_bytes()),
        ));
    }

    fn get_expects_inline_qos(&self) -> Option<bool> {
        self.iter()
            .find(|param| {
                param.parameter_id() == ParameterId::PidExpectsInlineQos
                    && !param.value().is_empty()
            })
            .map(|param| param.value()[0] != 0)
    }

    fn set_expects_inline_qos(&mut self, expects: bool) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidExpectsInlineQos);
        let value = if expects { SmallVec::from_slice(&[1]) } else { SmallVec::from_slice(&[0]) };
        self.add_parameter(make_parameter(ParameterId::PidExpectsInlineQos, value));
    }

    fn get_group_seq_num(&self) -> Option<u64> {
        self.iter()
            .find(|param| {
                param.parameter_id() == ParameterId::PidGroupSeqNum && param.value().len() == 8
            })
            .and_then(|param| param.value().try_into().ok())
            .map(u64::from_le_bytes)
    }

    fn set_group_seq_num(&mut self, seq_num: u64) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidGroupSeqNum);
        self.add_parameter(make_parameter(
            ParameterId::PidGroupSeqNum,
            SmallVec::from_slice(&seq_num.to_le_bytes()),
        ));
    }

    fn get_content_filter_info(&self) -> Option<Vec<u8>> {
        self.iter()
            .find(|param| param.parameter_id() == ParameterId::PidContentFilterInfo)
            .map(|param| param.value().to_vec())
    }

    fn set_content_filter_info(&mut self, info: Vec<u8>) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidContentFilterInfo);
        self.add_parameter(make_parameter(ParameterId::PidContentFilterInfo, info));
    }

    fn get_coherent_set(&self) -> Option<Vec<u8>> {
        self.iter()
            .find(|param| param.parameter_id() == ParameterId::PidCoherentSet)
            .map(|param| param.value().to_vec())
    }

    fn set_coherent_set(&mut self, coherent_set: Vec<u8>) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidCoherentSet);
        self.add_parameter(make_parameter(ParameterId::PidCoherentSet, coherent_set));
    }

    fn get_directed_write(&self) -> Option<Vec<u8>> {
        self.iter()
            .find(|param| param.parameter_id() == ParameterId::PidDirectedWrite)
            .map(|param| param.value().to_vec())
    }

    fn set_directed_write(&mut self, directed_write: Vec<u8>) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidDirectedWrite);
        self.add_parameter(make_parameter(ParameterId::PidDirectedWrite, directed_write));
    }

    fn get_original_writer_info(&self) -> Option<Vec<u8>> {
        self.iter()
            .find(|param| param.parameter_id() == ParameterId::PidOriginalWriterInfo)
            .map(|param| param.value().to_vec())
    }

    fn set_original_writer_info(&mut self, info: Vec<u8>) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidOriginalWriterInfo);
        self.add_parameter(make_parameter(ParameterId::PidOriginalWriterInfo, info));
    }

    fn get_group_coherent_set(&self) -> Option<Vec<u8>> {
        self.iter()
            .find(|param| param.parameter_id() == ParameterId::PidGroupCoherentSet)
            .map(|param| param.value().to_vec())
    }

    fn set_group_coherent_set(&mut self, coherent_set: Vec<u8>) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidGroupCoherentSet);
        self.add_parameter(make_parameter(ParameterId::PidGroupCoherentSet, coherent_set));
    }

    fn get_writer_group_info(&self) -> Option<Vec<u8>> {
        self.iter()
            .find(|param| param.parameter_id() == ParameterId::PidWriterGroupInfo)
            .map(|param| param.value().to_vec())
    }

    fn set_writer_group_info(&mut self, info: Vec<u8>) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidWriterGroupInfo);
        self.add_parameter(make_parameter(ParameterId::PidWriterGroupInfo, info));
    }

    fn get_secure_writer_group_info(&self) -> Option<Vec<u8>> {
        self.iter()
            .find(|param| param.parameter_id() == ParameterId::PidSecureWriterGroupInfo)
            .map(|param| param.value().to_vec())
    }

    fn set_secure_writer_group_info(&mut self, info: Vec<u8>) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidSecureWriterGroupInfo);
        self.add_parameter(make_parameter(ParameterId::PidSecureWriterGroupInfo, info));
    }

    fn get_data_representation(&self) -> Option<DataRepresentationQosPolicy> {
        self.iter()
            .find(|param| param.parameter_id() == ParameterId::PidDataRepresentation)
            .and_then(|param| {
                let bytes = param.value();
                if bytes.len() < 4 {
                    return None;
                }

                let sequence_length =
                    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;

                if bytes.len() < 4 + (sequence_length * 2) {
                    return None;
                }

                let mut representations = Vec::with_capacity(sequence_length);
                for i in 0..sequence_length {
                    let offset = 4 + (i * 2);
                    if offset + 1 >= bytes.len() {
                        return None;
                    }
                    let id = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
                    match id {
                        0 => representations.push(DataRepresentationId::XcdrDataRepresentation),
                        1 => representations.push(DataRepresentationId::XmlDataRepresentation),
                        2 => representations.push(DataRepresentationId::Xcdr2DataRepresentation),
                        _ => {}
                    }
                }

                if representations.is_empty() {
                    None
                } else {
                    Some(DataRepresentationQosPolicy { value: representations })
                }
            })
    }

    fn set_data_representation(&mut self, data_representation: DataRepresentationQosPolicy) {
        self.retain_parameters(|p| p.parameter_id() != ParameterId::PidDataRepresentation);

        if data_representation.value.is_empty() {
            return;
        }

        let mut serialized = Vec::with_capacity(4 + data_representation.value.len() * 2);
        let length = data_representation.value.len() as u32;
        serialized.extend_from_slice(&length.to_le_bytes());

        for rep in &data_representation.value {
            serialized.extend_from_slice(&(*rep as u16).to_le_bytes());
        }

        self.add_parameter(make_parameter(ParameterId::PidDataRepresentation, serialized));
    }
}
