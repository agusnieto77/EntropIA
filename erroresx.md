$env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path", "User") + ";C:\Users\agusn\.cargo\bin"; pnpm --filter @entropia/desktop tauri dev

> @entropia/desktop@0.0.9 tauri F:\POSITRON\EntropIA\apps\desktop
> tauri "dev"

     Running BeforeDevCommand (`pnpm --filter @entropia/desktop dev`)

> @entropia/desktop@0.0.9 dev F:\POSITRON\EntropIA\apps\desktop
> vite


  VITE v6.4.2  ready in 1490 ms

  ➜  Local:   http://localhost:1420/
  ➜  Network: use --host to expose
     Running DevCommand (`cargo  run --no-default-features --features paddle-ocr --color always --`)
        Info Watching F:\POSITRON\EntropIA\apps\desktop\src-tauri for changes...
   Compiling entropia-desktop v0.0.9 (F:\POSITRON\EntropIA\apps\desktop\src-tauri)
error[E0308]: mismatched types                              
   --> src\ocr\mod.rs:965:42
    |
965 |                         layout.push_page(page_idx + 1, &vl_output);
    |                                --------- ^^^^^^^^^^^^ expected `u32`, found `usize`
    |                                |
    |                                arguments to this method are incorrect
    |
note: method defined here
   --> src\ocr\mod.rs:102:8
    |
102 |     fn push_page(&mut self, page: u32, output: &PaddleVlOutput) {
    |        ^^^^^^^^^            ---------
help: you can convert a `usize` to a `u32` and panic if the converted value doesn't fit
    |
965 |                         layout.push_page((page_idx + 1).try_into().unwrap(), &vl_output);
    |                                          +            +++++++++++++++++++++

error[E0308]: mismatched types                              
   --> src\ocr\mod.rs:967:83
    |
967 |                         layout_payload = Some(LayoutPersistencePayload::from_page(page_idx + 1, &vl_output));
    |                                               ----------------------------------- ^^^^^^^^^^^^ expected `u32`, found `usize`
    |                                               |
    |                                               arguments to this function are incorrect
    |
note: associated function defined here
   --> src\ocr\mod.rs:90:8
    |
90  |     fn from_page(page: u32, output: &PaddleVlOutput) -> Self {
    |        ^^^^^^^^^ ---------
help: you can convert a `usize` to a `u32` and panic if the converted value doesn't fit
    |
967 |                         layout_payload = Some(LayoutPersistencePayload::from_page((page_idx + 1).try_into().unwrap(), &vl_output));
    |                                                                                   +            +++++++++++++++++++++

error[E0308]: mismatched types                              
    --> src\ocr\mod.rs:1146:24
     |
1146 |                       Ok(ProcessedOcrOutput {
     |  _____________________--_^
     | |                     |
     | |                     arguments to this enum variant are incorrect
1147 | |                         ocr: ocr_output_from_paddlevl(&vl_output),
1148 | |                         layout: Some(LayoutPersistencePayload::from_page(1, &vl_output)),
1149 | |                     })
     | |_____________________^ expected `OcrOutput`, found `ProcessedOcrOutput`
     |
help: the type constructed contains `ProcessedOcrOutput` due to the type of the argument passed
    --> src\ocr\mod.rs:1146:21
     |
1146 |                        Ok(ProcessedOcrOutput {
     |   _____________________^  -
     |  |________________________|
1147 | ||                         ocr: ocr_output_from_paddlevl(&vl_output),
1148 | ||                         layout: Some(LayoutPersistencePayload::from_page(1, &vl_output)),
1149 | ||                     })
     | ||_____________________-^
     | |______________________|
     |                        this argument influences the type of `Ok`
note: tuple variant defined here
    --> /rustc/6b00bc3880198600130e1cf62b8f8a93494488cc\library\core\src\result.rs:552:5

error[E0308]: mismatched types                              
    --> src\ocr\mod.rs:1156:21
     |
1156 | /                     provider_clone
1157 | |                         .recognize(&bytes_owned)
1158 | |                         .map(|ocr| ProcessedOcrOutput { ocr, layout: None })
1159 | |                         .map_err(|e| format!("OCR inference failed: {e}"))
     | |__________________________________________________________________________^ expected `Result<OcrOutput, String>`, found `Result<ProcessedOcrOutput, String>`
     |
     = note: expected enum `Result<OcrOutput, _>`
                found enum `Result<ProcessedOcrOutput, _>`
note: return type inferred to be `Result<OcrOutput, std::string::String>` here
    --> src\ocr\mod.rs:1121:24
     |
1121 |                   return provider_clone
     |  ________________________^
1122 | |                     .recognize(&bytes_owned)
1123 | |                     .map_err(|e| format!("OCR inference failed: {e}"));
     | |______________________________________________________________________^

error[E0308]: mismatched types                              
    --> src\ocr\mod.rs:1167:19
     |
1167 |         return Ok(output);
     |                -- ^^^^^^ expected `ProcessedOcrOutput`, found `OcrOutput`
     |                |
     |                arguments to this enum variant are incorrect
     |
help: the type constructed contains `OcrOutput` due to the type of the argument passed
    --> src\ocr\mod.rs:1167:16
     |
1167 |         return Ok(output);
     |                ^^^------^
     |                   |
     |                   this argument influences the type of `Ok`
note: tuple variant defined here
    --> /rustc/6b00bc3880198600130e1cf62b8f8a93494488cc\library\core\src\result.rs:552:5

For more information about this error, try `rustc --explain E0308`.
error: could not compile `entropia-desktop` (lib) due to 5 previous errors