macro_rules! cargo_command {
    ($command: ident, $options: ident) => {{
        paste::paste! {
            use cargo_options::[<$command:camel>];

            let mut cargo = serde_json::from_str::<[<$command:camel>]>($options)
                .context("CARGO_OPTIONS 格式错误!")?;

            // 始终显示颜色
            cargo.color = Some("always".into());
            cargo.command()
        }
    }};

    ($command: ident, $options: ident, $use_zigbuild: ident) => {{
        paste::paste! {
            use cargo_options::[<$command:camel>];
            use cargo_options::CargoOptionsExt;

            let mut cargo = serde_json::from_str::<[<$command:camel>]>($options)
                .context("CARGO_OPTIONS 格式错误!")?;

            // 始终显示颜色
            cargo.color = Some("always".into());

            // 使用 cargo-zigbuild 构建

            let cargo_command = match std::env::var_os("CARGO") {
                Some(cargo) => cargo.into(),
                None => PathBuf::from("cargo"),
            };

            let mut cmd = Command::new(cargo_command);
            cmd.env_remove("CARGO");

            if $use_zigbuild {
                cmd.arg("zigbuild");
            } else {
                cmd.arg(stringify!([<$command:lower>]));
            }

            cmd.args(cargo.options());
            cmd
        }
    }};
}
