#[derive(speedy::Readable, speedy::Writable, DdsType)]
pub struct HelloWorld {
    pub index: u32,
    pub message: String,
}
