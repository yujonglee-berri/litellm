use litellm_operation::InteractionWireProtocol;

struct InvalidProtocol;

impl InteractionWireProtocol for InvalidProtocol {
    type Request = ();
    type ClientMessage = ();
    type ServerOutput = String;
}

fn main() {}
