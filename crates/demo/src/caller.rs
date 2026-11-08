/// Wraps one async `Caller` method in a command file. Adds the method to
/// `Caller` and generates the file's `PARAMS`, `RETURNS` and
/// `call(caller, args)`, which parses each string arg with `FromStr`, awaits
/// the method and prints its return value, if any.
macro_rules! command {
    // Awaits a call, printing its result only when there's a `-> $ret`.
    (@run $call:expr, $ret:ty) => {
        println!("{:?}", $call.await)
    };
    (@run $call:expr) => {
        $call.await
    };

    (
        $(#[$meta:meta])*
        $vis:vis async fn $name:ident(&self $(, $arg:ident: $arg_ty:ty)* $(,)?) $(-> $ret:ty)? $body:block
    ) => {
        impl crate::Caller {
            $(#[$meta])*
            $vis async fn $name(&self $(, $arg: $arg_ty)*) $(-> $ret)? $body
        }

        pub const PARAMS: &[&str] = &[$(concat!(stringify!($arg), ": ", stringify!($arg_ty))),*];
        pub const RETURNS: &str = concat!("" $(, "-> ", stringify!($ret))?);

        pub async fn call(caller: &crate::Caller, args: &[String]) -> Result<(), String> {
            let [$($arg),*] = args else {
                let params: &[&str] = &[$(stringify!($arg)),*];
                return Err(format!(
                    "{} takes {} argument(s) ({}), got {}",
                    stringify!($name), params.len(), params.join(", "), args.len(),
                ));
            };
            $(
                let $arg: $arg_ty = $arg.parse().map_err(|e| {
                    format!("{}: can't parse {:?} as {}: {e}", stringify!($arg), $arg, stringify!($arg_ty))
                })?;
            )*
            command!(@run caller.$name($($arg),*) $(, $ret)?);
            Ok(())
        }
    };
}

/// Declares one module per command file and generates `Caller::FUNCTIONS` and
/// `Caller::call(name, args)`. The file name is the command name.
macro_rules! commands {
    ($($file:ident),* $(,)?) => {
        $(mod $file;)*

        impl crate::Caller {
            /// (name, parameters, return type or "")
            pub const FUNCTIONS: &[(&str, &[&str], &str)] = &[
                $((stringify!($file), $file::PARAMS, $file::RETURNS)),*
            ];

            pub async fn call(&self, name: &str, args: &[String]) -> Result<(), String> {
                match name {
                    $(stringify!($file) => $file::call(self, args).await,)*
                    _ => Err(format!("unknown function {name:?}")),
                }
            }
        }
    };
}
