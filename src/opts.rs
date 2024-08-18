use crate::{CargoCommand, Opts, SubCommand};
use clap::Parser;
use once_cell::sync::OnceCell;

static OPTS: OnceCell<Opts> = OnceCell::new();

pub(crate) fn get() -> &'static Opts {
    OPTS.get_or_init(|| {
        let opts = CargoCommand::parse();

        let SubCommand::LineTest(mut opts) = opts.subcmd;

        if opts.no_run {
            opts.show_commands = true;
        }

        opts
    })
}
