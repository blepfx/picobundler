use bpaf::{Parser, construct};
use owo_colors::OwoColorize;
use std::str::FromStr;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ArgsVst3 {
    None,
    Gpl,
    Proprietary,
}

#[derive(Debug)]
pub struct ArgsAppleSign {
    pub identity: String,
    pub team: String,
    pub username: String,
    pub password: String,
}

impl FromStr for ArgsVst3 {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "gpl" => Ok(ArgsVst3::Gpl),
            "proprietary" => Ok(ArgsVst3::Proprietary),
            _ => Err(format!(
                "use either {} or {} as VST3 SDK",
                "GPL".bold().bright_cyan(),
                "PROPRIETARY".bold().bright_green()
            )),
        }
    }
}

#[derive(Debug)]
pub struct ArgsBuild {
    pub packages: Vec<String>,

    pub profile: Option<String>,
    pub target: Vec<String>,

    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
}

#[derive(Debug)]
pub struct Args {
    pub install: bool,
    pub verbose: bool,

    pub codesign: Option<ArgsAppleSign>,

    pub build: ArgsBuild,
    pub vst3: ArgsVst3,
    pub auv2: bool,
    pub clap: bool,
}

fn parser_build() -> impl Parser<ArgsBuild> {
    let packages = bpaf::long("package")
        .short('p')
        .argument("PACKAGE")
        .help("A list of packages to build")
        .many();

    let profile = bpaf::long("profile")
        .argument("PROFILE")
        .help("Build with the specified profile (release by default)")
        .optional();

    let target = bpaf::long("target")
        .argument("TARGET")
        .help("Build for the target triple")
        .many();

    let features = bpaf::long("features")
        .short('F')
        .argument("FEATURES")
        .help("List of features to use")
        .many();

    let all_features = bpaf::long("all-features")
        .switch()
        .help("Use all available features");
    let no_default_features = bpaf::long("no-default-features")
        .switch()
        .help("Do not use the default features");

    construct!(ArgsBuild {
        packages,
        profile,
        target,
        features,
        all_features,
        no_default_features,
    })
}

fn parser_codesign() -> impl Parser<ArgsAppleSign> {
    let identity = bpaf::long("sign-identity")
        .argument("IDENTITY")
        .help("The identity to use for signing");

    let team = bpaf::long("sign-team")
        .argument("TEAM")
        .help("The team to use for signing");

    let username = bpaf::long("sign-username")
        .argument("USERNAME")
        .help("The username to use for signing");

    let password = bpaf::long("sign-password")
        .argument("PASSWORD")
        .help("The password to use for signing");

    construct!(ArgsAppleSign {
        identity,
        team,
        username,
        password,
    })
}

fn parser_args() -> impl Parser<Args> {
    let build = parser_build();

    let install = bpaf::long("install")
        .switch()
        .help("Install built plugins to system locations");
    let verbose = bpaf::long("verbose")
        .short('v')
        .switch()
        .help("Enable verbose logging");

    let vst3 = bpaf::long("vst3")
        .argument("SDK")
        .adjacent()
        .help("Build VST3 plugin")
        .fallback(ArgsVst3::None);

    let codesign = parser_codesign().optional();

    let auv2 = bpaf::long("auv2").switch().help("Build AUv2 plugin");
    let clap = bpaf::long("clap").switch().help("Build CLAP plugin");

    construct!(Args {
        install,
        build,
        verbose,
        codesign,
        vst3,
        auv2,
        clap
    })
}

pub fn parse_args() -> Args {
    parser_args().to_options().run()
}
