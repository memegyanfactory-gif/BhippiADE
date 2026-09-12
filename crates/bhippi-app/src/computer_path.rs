//! Continuous pointer input. Only validated numeric coordinates enter the native bridge.

/// Shared by ordinary drags and creative strokes. The caller validates bounds first.
pub(crate) fn script(
    points: &[[i32; 2]],
    button: &str,
    duration_ms: u32,
) -> Result<String, String> {
    if points.len() < 2 || points.len() > bhippi_types::COMPUTER_MAX_PATH_POINTS {
        return Err("Mouse path point count is outside the supported range.".to_owned());
    }
    if !(bhippi_types::COMPUTER_PATH_SAMPLE_MS..=bhippi_types::COMPUTER_MAX_PATH_DURATION_MS)
        .contains(&duration_ms)
    {
        return Err("Mouse path duration is outside the supported range.".to_owned());
    }
    let (down, up) = match button {
        "left" => (2, 4),
        "right" => (8, 16),
        "middle" => (32, 64),
        _ => return Err("Unsupported mouse path button.".to_owned()),
    };
    let coordinates = points
        .iter()
        .flat_map(|p| p.iter())
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let sample_ms = bhippi_types::COMPUTER_PATH_SAMPLE_MS;
    Ok(format!(
        r#"
$ErrorActionPreference = 'Stop'
$source = @'
using System;
using System.Runtime.InteropServices;
using System.Diagnostics;
using System.Threading;
public class BhippiContinuousPointer {{
  [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr value);
  [DllImport("user32.dll", SetLastError=true)] static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] static extern short GetAsyncKeyState(int key);
  [DllImport("user32.dll")] static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
  [DllImport("user32.dll")] static extern bool SetThreadDesktop(IntPtr desktop);
  [DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr desktop);
  static void Move(int x, int y) {{
    if (!SetCursorPos(x, y)) throw new InvalidOperationException("Pointer movement was refused.");
  }}
  public static void Run() {{
    Exception failure = null;
    var thread = new Thread(() => {{
      IntPtr desktop = IntPtr.Zero;
      bool pressed = false;
      try {{
        desktop = OpenInputDesktop(0, false, 0x01FF);
        if (desktop == IntPtr.Zero || !SetThreadDesktop(desktop))
          throw new InvalidOperationException("The input desktop is unavailable.");
        SetThreadDpiAwarenessContext(new IntPtr(-4));
        int[] p = new int[] {{ {coordinates} }};
        int count = p.Length / 2;
        double[] lengths = new double[count];
        for (int i = 1; i < count; i++) {{
          double dx = (double)p[2*i] - p[2*i-2], dy = (double)p[2*i+1] - p[2*i-1];
          lengths[i] = lengths[i-1] + Math.Sqrt(dx*dx + dy*dy);
        }}
        Move(p[0], p[1]);
        Thread.Sleep({sample_ms});
        mouse_event({down}, 0, 0, 0, UIntPtr.Zero);
        pressed = true;
        var timer = Stopwatch.StartNew();
        int segment = 1;
        do {{
          if ((GetAsyncKeyState(27) & 0x8000) != 0)
            throw new OperationCanceledException("Drawing interrupted by Escape.");
          double progress = Math.Min(1.0, (double)timer.ElapsedMilliseconds / {duration_ms});
          double distance = progress * lengths[count-1];
          while (segment < count-1 && lengths[segment] < distance) {{
            Move(p[2*segment], p[2*segment+1]);
            segment++;
          }}
          double span = lengths[segment] - lengths[segment-1];
          double t = span == 0 ? 1 : (distance - lengths[segment-1]) / span;
          int x = (int)Math.Round(p[2*segment-2] + ((double)p[2*segment] - p[2*segment-2])*t);
          int y = (int)Math.Round(p[2*segment-1] + ((double)p[2*segment+1] - p[2*segment-1])*t);
          Move(x, y);
          Thread.Sleep({sample_ms});
        }} while (timer.ElapsedMilliseconds < {duration_ms});
        Move(p[p.Length-2], p[p.Length-1]);
      }} catch (Exception error) {{ failure = error; }}
      finally {{
        if (pressed) mouse_event({up}, 0, 0, 0, UIntPtr.Zero);
        if (desktop != IntPtr.Zero) CloseDesktop(desktop);
      }}
    }});
    thread.SetApartmentState(ApartmentState.STA);
    thread.Start();
    thread.Join();
    if (failure != null) throw failure;
  }}
}}
'@
Add-Type -TypeDefinition $source
[BhippiContinuousPointer]::Run()
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_compiles_the_real_bridge_without_sending_input() {
        let source = script(&[[0, 0], [10, 20], [30, 0]], "left", 200).unwrap();
        let compile_only = source.replace(
            "[BhippiContinuousPointer]::Run()",
            "Write-Output 'compiled'",
        );
        let result = crate::computer::run_powershell_output(&compile_only)
            .await
            .unwrap();
        assert_eq!(result.trim(), "compiled");
    }

    #[test]
    fn malformed_paths_are_rejected_before_building_native_code() {
        assert!(script(&[], "left", 100).is_err());
        assert!(script(&[[0, 0]; 129], "left", 100).is_err());
        assert!(script(&[[0, 0], [1, 1]], "left", u32::MAX).is_err());
        assert!(script(&[[0, 0], [1, 1]], "left; injected", 100).is_err());
    }

    #[test]
    fn curves_and_middle_drags_release_the_selected_button_even_on_failure() {
        let code = script(&[[-50, 0], [0, 30], [50, 0]], "middle", 600).unwrap();
        assert!(code.contains("-50,0,0,30,50,0"));
        assert!(code.contains("mouse_event(32"));
        assert!(code.contains("if (pressed) mouse_event(64"));
        assert!(code.contains("finally"));
        assert!(code.contains("GetAsyncKeyState(27)"));
    }
}
