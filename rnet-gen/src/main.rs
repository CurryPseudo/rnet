use std::{
    ffi::OsStr,
    io::{stdout, BufWriter, Write},
    path::PathBuf,
};

use anyhow::{anyhow, Context};
use heck::{CamelCase, MixedCase};
use libloading::Library;
use structopt::StructOpt;

use rnet::hidden::{GeneratorContext, LibDesc, VERSION};

/// A basic example
#[derive(StructOpt, Debug)]
#[structopt(name = "rnet-gen")]
struct Opt {
    /// Path to shared library or DLL
    #[structopt()]
    path: PathBuf,
}

type ReflectFn = extern "C" fn(usize, &mut LibDesc) -> bool;

const COMMON: &str = include_str!("common.cs");

fn generate_csharp_code(_opt: &Opt, name: &str, desc: LibDesc) -> anyhow::Result<()> {
    let stdout = stdout();
    let mut writer = BufWriter::new(stdout.lock());

    let common = COMMON
        .replace("__ClassName__", &name.to_camel_case())
        .replace("\"__LibName__\"", &format!("{:?}", name));
    let mut parts = common.splitn(2, "// __Remainder__");
    let prefix = parts.next().unwrap();
    let suffix = parts.next().unwrap();

    let mut extra_items = Vec::new();
    let mut add_item = |item: &str| extra_items.push(item.to_owned());
    let mut ctx = GeneratorContext::new(&mut add_item);

    writeln!(writer, "{}", prefix)?;

    // Write wrapper structs
    for struct_desc in desc.structs {
        writeln!(writer, "        public struct {} {{", struct_desc.name)?;
        for field_desc in struct_desc.fields {
            let net_ty = (field_desc.ty_.net_ty)().unwrap();
            writeln!(
                writer,
                "            public {} {};",
                net_ty,
                field_desc.name.to_mixed_case()
            )?;
        }
        writeln!(writer, "        }}")?;
    }

    // Write wrappers
    for fn_desc in desc.fns {
        let ret_net_ty = (fn_desc.ret_ty.net_ty)();
        writeln!(
            writer,
            "        public static {} {}(",
            if let Some(ret_ty) = ret_net_ty.as_deref() {
                ret_ty
            } else {
                "void"
            },
            fn_desc.name.to_camel_case()
        )?;
        for (i, arg) in fn_desc.args.iter().enumerate() {
            writeln!(
                writer,
                "            {} {}{}",
                (arg.ty_.base_ty)().unwrap(),
                arg.name.to_mixed_case(),
                if i + 1 == fn_desc.args.len() { "" } else { "," }
            )?;
        }
        writeln!(writer, "        ) {{")?;

        // Generate function call
        let mut call_code = String::new();
        call_code += &format!("_Fn{}(", fn_desc.name.to_camel_case());
        for (i, arg) in fn_desc.args.iter().enumerate() {
            call_code += &format!(
                "{}{}",
                (arg.ty_.marshal_in.unwrap())(&mut ctx, &arg.name.to_mixed_case()),
                if i + 1 == fn_desc.args.len() { "" } else { "," }
            );
        }
        call_code += ")";

        // Wrap call code in return type marshalling
        let call_code = (fn_desc.ret_ty.marshal_out.unwrap())(&mut ctx, &call_code);

        if ret_net_ty.is_none() {
            writeln!(writer, "            {};", call_code)?;
        } else {
            writeln!(writer, "            return {};", call_code)?;
        }

        writeln!(writer, "        }}")?;
    }

    // Write raw structs
    for struct_desc in desc.structs {
        writeln!(writer, "        [StructLayout(LayoutKind.Sequential)]")?;
        writeln!(
            writer,
            "        private struct _Struct{} {{",
            struct_desc.name
        )?;
        for field_desc in struct_desc.fields {
            let raw_ty = (field_desc.ty_.raw_ty)().unwrap();
            writeln!(
                writer,
                "            public {} {};",
                raw_ty,
                field_desc.name.to_mixed_case()
            )?;
        }
        writeln!(
            writer,
            "            public static _Struct{} Encode({} structArg) {{",
            struct_desc.name, struct_desc.name
        )?;
        writeln!(
            writer,
            "                return new _Struct{} {{",
            struct_desc.name
        )?;
        for (i, field_desc) in struct_desc.fields.iter().enumerate() {
            let field_name = field_desc.name.to_mixed_case();
            let init_expr = (field_desc.ty_.marshal_in.unwrap())(
                &mut ctx,
                &format!("structArg.{}", field_name),
            );
            writeln!(
                writer,
                "                    {} = {}{}",
                field_name,
                init_expr,
                if i + 1 == struct_desc.fields.len() {
                    ""
                } else {
                    ","
                }
            )?;
        }
        writeln!(writer, "                }};")?;
        writeln!(writer, "            }}")?;
        writeln!(
            writer,
            "            public {} Decode() {{",
            struct_desc.name
        )?;
        writeln!(writer, "                return new {} {{", struct_desc.name)?;
        for (i, field_desc) in struct_desc.fields.iter().enumerate() {
            let field_name = field_desc.name.to_mixed_case();
            let init_expr =
                (field_desc.ty_.marshal_out.unwrap())(&mut ctx, &format!("this.{}", field_name));
            writeln!(
                writer,
                "                    {} = {}{}",
                field_name,
                init_expr,
                if i + 1 == struct_desc.fields.len() {
                    ""
                } else {
                    ","
                }
            )?;
        }
        writeln!(writer, "                }};")?;
        writeln!(writer, "            }}")?;
        writeln!(writer, "        }}")?;
    }

    // Write imports
    for fn_desc in desc.fns {
        let maybe_ret_ty = fn_desc.ret_ty;
        writeln!(
            writer,
            "        [DllImport({:?}, EntryPoint = {:?}, CallingConvention = CallingConvention.Cdecl)]",
            name, fn_desc.name
        )?;
        writeln!(
            writer,
            "        private static extern {} _Fn{}(",
            if let Some(ret_ty) = (maybe_ret_ty.raw_ty)() {
                ret_ty
            } else {
                "void".into()
            },
            fn_desc.name.to_camel_case()
        )?;
        for (i, arg) in fn_desc.args.iter().enumerate() {
            writeln!(
                writer,
                "            {} {}{}",
                (arg.ty_.raw_ty)().unwrap(),
                arg.name.to_mixed_case(),
                if i + 1 == fn_desc.args.len() { "" } else { "," }
            )?;
        }
        writeln!(writer, "        );")?;
    }

    // Add extra items
    for item in extra_items {
        writeln!(writer, "        {}", item)?;
    }

    writeln!(writer, "{}", suffix)?;
    writer.flush()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    unsafe {
        let lib = Library::new(&opt.path).context("Failed to load library")?;
        let symbol = lib
            .get::<ReflectFn>(b"rnet_reflect")
            .context("Library does not link to `rnet-core`")?;
        let mut desc = LibDesc::default();
        if !symbol(VERSION, &mut desc) {
            return Err(anyhow!(
                "Library was built against an incompatible version of `rnet-core`"
            ));
        }

        let mut name = opt.path.file_stem().unwrap().to_str().unwrap();
        if opt.path.extension() != Some(OsStr::new("dll")) {
            if let Some(remainder) = name.strip_prefix("lib") {
                name = remainder;
            }
        }

        generate_csharp_code(&opt, name, desc)?;
    }
    eprintln!("Done.");
    Ok(())
}
