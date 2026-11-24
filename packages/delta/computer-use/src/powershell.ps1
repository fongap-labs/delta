$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Add-Type -AssemblyName System.Drawing
$payload = ConvertFrom-Json -InputObject ([Console]::In.ReadToEnd())

$nativeSource = @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class DeltaDesktopNative {
    private const int SM_XVIRTUALSCREEN = 76;
    private const int SM_YVIRTUALSCREEN = 77;
    private const int SM_CXVIRTUALSCREEN = 78;
    private const int SM_CYVIRTUALSCREEN = 79;
    private const uint INPUT_KEYBOARD = 1;
    private const uint KEYEVENTF_KEYUP = 0x0002;
    private const uint KEYEVENTF_UNICODE = 0x0004;
    private const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
    private const uint MOUSEEVENTF_LEFTUP = 0x0004;
    private const uint MOUSEEVENTF_RIGHTDOWN = 0x0008;
    private const uint MOUSEEVENTF_RIGHTUP = 0x0010;
    private const uint MOUSEEVENTF_MIDDLEDOWN = 0x0020;
    private const uint MOUSEEVENTF_MIDDLEUP = 0x0040;
    private const uint MOUSEEVENTF_WHEEL = 0x0800;

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    private struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Sequential)]
    private struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Sequential)]
    private struct HARDWAREINPUT { public uint uMsg; public ushort wParamL; public ushort wParamH; }
    [StructLayout(LayoutKind.Explicit)]
    private struct INPUTUNION {
        [FieldOffset(0)] public MOUSEINPUT mi;
        [FieldOffset(0)] public KEYBDINPUT ki;
        [FieldOffset(0)] public HARDWAREINPUT hi;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct INPUT { public uint type; public INPUTUNION U; }
    private delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lParam);

    [DllImport("user32.dll")] private static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] private static extern void mouse_event(uint flags, uint dx, uint dy, int data, IntPtr extraInfo);
    [DllImport("user32.dll", SetLastError = true)] private static extern uint SendInput(uint count, INPUT[] inputs, int size);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowTextLengthW(IntPtr hwnd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowTextW(IntPtr hwnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

    public static Rectangle VirtualBounds() {
        int left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        int top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        int width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        int height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        if (width < 1 || height < 1 || width > 32768 || height > 32768 || (long)width * height > 64000000L) throw new InvalidOperationException("Invalid desktop dimensions");
        return new Rectangle(left, top, width, height);
    }

    private static bool Move(int x, int y) {
        Rectangle bounds = VirtualBounds();
        if (x < 0 || y < 0 || x >= bounds.Width || y >= bounds.Height) return false;
        return SetCursorPos(bounds.Left + x, bounds.Top + y);
    }

    public static bool Click(int x, int y, string button, int count) {
        if (count < 1 || count > 2 || !Move(x, y)) return false;
        uint down, up;
        if (button == "left") { down = MOUSEEVENTF_LEFTDOWN; up = MOUSEEVENTF_LEFTUP; }
        else if (button == "right") { down = MOUSEEVENTF_RIGHTDOWN; up = MOUSEEVENTF_RIGHTUP; }
        else if (button == "middle") { down = MOUSEEVENTF_MIDDLEDOWN; up = MOUSEEVENTF_MIDDLEUP; }
        else return false;
        for (int i = 0; i < count; i++) {
            mouse_event(down, 0, 0, 0, IntPtr.Zero);
            mouse_event(up, 0, 0, 0, IntPtr.Zero);
            if (i + 1 < count) Thread.Sleep(90);
        }
        return true;
    }

    public static bool Scroll(int x, int y, int delta) {
        if (delta == 0 || delta < -1200 || delta > 1200 || !Move(x, y)) return false;
        mouse_event(MOUSEEVENTF_WHEEL, 0, 0, delta, IntPtr.Zero);
        return true;
    }

    private static INPUT Keyboard(ushort virtualKey, ushort scan, uint flags) {
        INPUT input = new INPUT();
        input.type = INPUT_KEYBOARD;
        input.U.ki = new KEYBDINPUT { wVk = virtualKey, wScan = scan, dwFlags = flags, time = 0, dwExtraInfo = IntPtr.Zero };
        return input;
    }

    public static bool TypeText(string text) {
        if (text == null || text.Length > 1000) return false;
        List<INPUT> inputs = new List<INPUT>();
        foreach (char value in text) {
            if (value == '\r') continue;
            if (value == '\n') {
                inputs.Add(Keyboard(0x0D, 0, 0));
                inputs.Add(Keyboard(0x0D, 0, KEYEVENTF_KEYUP));
            } else if (value == '\t') {
                inputs.Add(Keyboard(0x09, 0, 0));
                inputs.Add(Keyboard(0x09, 0, KEYEVENTF_KEYUP));
            } else {
                inputs.Add(Keyboard(0, value, KEYEVENTF_UNICODE));
                inputs.Add(Keyboard(0, value, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
            }
        }
        if (inputs.Count == 0) return false;
        return SendInput((uint)inputs.Count, inputs.ToArray(), Marshal.SizeOf(typeof(INPUT))) == inputs.Count;
    }

    private static ushort KeyCode(string name) {
        string key = name.ToUpperInvariant();
        if (key.Length == 1 && key[0] >= 'A' && key[0] <= 'Z') return (ushort)key[0];
        if (key.Length == 1 && key[0] >= '0' && key[0] <= '9') return (ushort)key[0];
        int function;
        if (key.Length >= 2 && key[0] == 'F' && Int32.TryParse(key.Substring(1), out function) && function >= 1 && function <= 12) return (ushort)(0x70 + function - 1);
        switch (key) {
            case "ENTER": case "RETURN": return 0x0D;
            case "ESC": case "ESCAPE": return 0x1B;
            case "TAB": return 0x09;
            case "SPACE": return 0x20;
            case "BACKSPACE": return 0x08;
            case "DELETE": case "DEL": return 0x2E;
            case "INSERT": case "INS": return 0x2D;
            case "HOME": return 0x24;
            case "END": return 0x23;
            case "PAGEUP": return 0x21;
            case "PAGEDOWN": return 0x22;
            case "UP": case "ARROWUP": return 0x26;
            case "DOWN": case "ARROWDOWN": return 0x28;
            case "LEFT": case "ARROWLEFT": return 0x25;
            case "RIGHT": case "ARROWRIGHT": return 0x27;
            case "CTRL": case "CONTROL": return 0x11;
            case "ALT": return 0x12;
            case "SHIFT": return 0x10;
            case "WIN": case "WINDOWS": return 0x5B;
            default: return 0;
        }
    }

    public static bool Press(string key) {
        string[] parts = key.Split('+');
        if (parts.Length < 1 || parts.Length > 3) return false;
        List<ushort> modifiers = new List<ushort>();
        for (int i = 0; i < parts.Length - 1; i++) {
            ushort modifier = KeyCode(parts[i]);
            if (modifier != 0x11 && modifier != 0x12 && modifier != 0x10 && modifier != 0x5B) return false;
            if (modifiers.Contains(modifier)) return false;
            modifiers.Add(modifier);
        }
        ushort main = KeyCode(parts[parts.Length - 1]);
        if (main == 0 || (parts.Length == 1 && (main == 0x11 || main == 0x12 || main == 0x10 || main == 0x5B))) return false;
        List<INPUT> inputs = new List<INPUT>();
        foreach (ushort modifier in modifiers) inputs.Add(Keyboard(modifier, 0, 0));
        inputs.Add(Keyboard(main, 0, 0));
        inputs.Add(Keyboard(main, 0, KEYEVENTF_KEYUP));
        for (int i = modifiers.Count - 1; i >= 0; i--) inputs.Add(Keyboard(modifiers[i], 0, KEYEVENTF_KEYUP));
        return SendInput((uint)inputs.Count, inputs.ToArray(), Marshal.SizeOf(typeof(INPUT))) == inputs.Count;
    }

    public static object[] ListWindows() {
        List<object> rows = new List<object>();
        EnumWindows((hwnd, lParam) => {
            if (!IsWindowVisible(hwnd)) return true;
            int length = GetWindowTextLengthW(hwnd);
            if (length <= 0) return true;
            StringBuilder title = new StringBuilder(Math.Min(length + 1, 2048));
            GetWindowTextW(hwnd, title, title.Capacity);
            string text = title.ToString().Trim();
            if (text.Length == 0) return true;
            RECT rect;
            if (!GetWindowRect(hwnd, out rect)) return true;
            uint pid;
            GetWindowThreadProcessId(hwnd, out pid);
            string processName = "";
            try { processName = Process.GetProcessById((int)pid).ProcessName; } catch { }
            rows.Add(new {
                windowId = hwnd.ToInt64().ToString("x"),
                title = text.Length > 240 ? text.Substring(0, 240) : text,
                processName = processName.Length > 100 ? processName.Substring(0, 100) : processName,
                left = rect.Left, top = rect.Top, width = rect.Right - rect.Left, height = rect.Bottom - rect.Top
            });
            return rows.Count < 80;
        }, IntPtr.Zero);
        return rows.ToArray();
    }
}
'@
Add-Type -TypeDefinition $nativeSource -ReferencedAssemblies 'System.Drawing'

function Get-Capture {
    $bounds = [DeltaDesktopNative]::VirtualBounds()
    $scale = [Math]::Min(1.0, [Math]::Min(1920.0 / $bounds.Width, 1080.0 / $bounds.Height))
    $width = [Math]::Max(1, [int][Math]::Floor($bounds.Width * $scale))
    $height = [Math]::Max(1, [int][Math]::Floor($bounds.Height * $scale))
    $source = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
    $sourceGraphics = [System.Drawing.Graphics]::FromImage($source)
    $target = $source
    $targetGraphics = $null
    $stream = [System.IO.MemoryStream]::new()
    try {
        $sourceGraphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bounds.Size)
        if ($scale -lt 1.0) {
            $target = [System.Drawing.Bitmap]::new($width, $height)
            $targetGraphics = [System.Drawing.Graphics]::FromImage($target)
            $targetGraphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $targetGraphics.DrawImage($source, 0, 0, $width, $height)
        }
        $target.Save($stream, [System.Drawing.Imaging.ImageFormat]::Jpeg)
        return @{
            image = [Convert]::ToBase64String($stream.ToArray())
            screen = @{ left = $bounds.Left; top = $bounds.Top; width = $bounds.Width; height = $bounds.Height; imageWidth = $width; imageHeight = $height; scale = $scale }
        }
    } finally {
        if ($targetGraphics) { $targetGraphics.Dispose() }
        if ($target -ne $source) { $target.Dispose() }
        $sourceGraphics.Dispose()
        $source.Dispose()
        $stream.Dispose()
    }
}

$windows = @()
$capture = $null
switch ([string]$payload.action) {
    'screenshot' { $capture = Get-Capture }
    'list_windows' { $windows = @([DeltaDesktopNative]::ListWindows()) }
    'click' {
        if (-not [DeltaDesktopNative]::Click([int]$payload.x, [int]$payload.y, [string]$payload.button, [int]$payload.count)) { throw 'Desktop click was refused.' }
        $capture = Get-Capture
    }
    'type' {
        if (-not [DeltaDesktopNative]::TypeText([string]$payload.text)) { throw 'Desktop text entry was refused.' }
        $capture = Get-Capture
    }
    'press' {
        if (-not [DeltaDesktopNative]::Press([string]$payload.key)) { throw 'Desktop key press was refused.' }
        $capture = Get-Capture
    }
    'scroll' {
        if (-not [DeltaDesktopNative]::Scroll([int]$payload.x, [int]$payload.y, [int]$payload.delta)) { throw 'Desktop scroll was refused.' }
        $capture = Get-Capture
    }
    default { throw 'Unknown Delta computer-use action.' }
}

$result = [ordered]@{ status = 'success'; action = [string]$payload.action; windows = $windows; image = ''; screen = $null }
if ($capture) {
    $result.image = $capture.image
    $result.screen = $capture.screen
}
[Console]::Out.Write((ConvertTo-Json -InputObject $result -Compress -Depth 8))
