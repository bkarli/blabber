use iroh_docs::DocTicket;

pub struct Room {
    name: String,
    ticket: DocTicket,
}

impl Room {
   
    pub fn new(name: impl Into<String>) /*-> Self*/ {
        let name = name.into();
        todo!();

    }

}
