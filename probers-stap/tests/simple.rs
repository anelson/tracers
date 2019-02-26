use failure::Fallible;
use probers_core::*;
use probers_stap::*;

fn main() -> Fallible<()> {
    let provider = StapTracer::define_provider("simple", |mut b| {
        b.add_probe::<(&str, u32)>("probe2")?;
        Ok(b)
    })?;

    let probe = provider.get_probe::<(&str, u32)>("probe2")?;

    assert_eq!(false, probe.is_enabled());

    Ok(())
}
