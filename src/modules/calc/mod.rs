// Live calculations for the screens: the client sends what the person has typed so far and gets
// back every figure, toggle counterpart and "what stops Save" for it. Nothing is read or written;
// the formulas live in the domain engines (`purchases::charges`, `purchases::pricing`, ...) and
// this module only shapes requests and replies. The save paths call the same engines, so a
// preview can never disagree with what is stored.
pub mod receipt;
pub mod routes;
