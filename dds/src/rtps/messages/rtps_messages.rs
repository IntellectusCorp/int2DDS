use speedy::{Context, Writable, Writer};

use crate::rtps::messages::{header::Header, submessage::Submessage};

// pub type SubmessageFlag = bool;
// pub type EndiannessFlag = SubmessageFlag;
// pub type LengthFlag = SubmessageFlag;
// pub type TimestampFlag = SubmessageFlag;
// pub type UExtensionFlag = SubmessageFlag;
// pub type WExtensionFlag = SubmessageFlag;
// pub type ChecksumFlags = [SubmessageFlag; 2];
// pub type ParametersFlag = SubmessageFlag;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RtpsMessage {
    pub header: Header,
    pub submessages: Vec<Submessage>,
}

impl RtpsMessage {
    pub(crate) fn new(header: Header) -> Self {
        Self { header, submessages: vec![] }
    }

    pub(crate) fn add_submessage(&mut self, submessage: Submessage) {
        self.submessages.push(submessage);
    }
}

impl<C: Context> Writable<C> for RtpsMessage {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_value(&self.header)?;
        for x in &self.submessages {
            writer.write_value(&x)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {}
