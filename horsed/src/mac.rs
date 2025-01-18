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
}
