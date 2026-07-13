use crate::space::Space;


pub struct Invite {
    space_name: String,
    info_ticket: String,
    member_ticket: String,
}


impl Invite {

    pub fn from_space(space: &Space) -> Self {
        Self {
            space_name: String::from(""),
            info_ticket: String::from(""),
            member_ticket: String::from(""),
        }
    }
}
