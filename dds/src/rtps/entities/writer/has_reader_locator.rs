#![allow(dead_code)]
#![allow(unused_variables)]

use crate::rtps::{common::locator::Locator, entities::writer::reader_locator::ReaderLocator};

pub(crate) trait HasReaderLocator {
    fn reader_locator(&self) -> Vec<ReaderLocator>;
    fn reader_locator_add(&self, a_locator: ReaderLocator);
    fn reader_locator_remove(&self, a_locator: Locator);
}
