using Avalonia;
using Avalonia.Controls;
using Avalonia.Media.Imaging;
using Avalonia.Platform;
using NxDesk.Client.ViewModels;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using Avalonia.Threading;
using System;

namespace NxDesk.Client.Views;

public partial class RemoteView : UserControl
{
    private WriteableBitmap? _bitmap;
    private bool _isRendering = false;

    public RemoteView()
    {
        InitializeComponent();
        DataContextChanged += OnDataContextChanged;
    }

    private void OnDataContextChanged(object? sender, EventArgs e)
    {
        if (DataContext is MainWindowViewModel vm)
        {
            vm.OnFrameReceived += OnFrameReceived;
            vm.OnSessionClosed += OnSessionClosed;
        }
    }

    private void OnSessionClosed()
    {
        Dispatcher.UIThread.Post(() => 
        {
            VideoImage.Source = null;
            _bitmap?.Dispose();
            _bitmap = null;
            _isRendering = false; // Also reset rendering guard
        });
    }

    private void OnFrameReceived(NxDesk.Application.DTOs.VideoFrameDto frame)
    {
        if (_isRendering) return;
        _isRendering = true;

        // Decode frame in background to keep UI thread free
        Task.Run(() =>
        {
            try
            {
                if (frame.IsCompressed)
                {
                    // Handle JPEG compressed frames
                    using (var ms = new System.IO.MemoryStream(frame.Pixels))
                    {
                        // Decoding is CPU intensive
                        var bitmap = new Bitmap(ms);
                        
                        // Update UI with the decoded bitmap
                        Dispatcher.UIThread.Post(() =>
                        {
                            VideoImage.Source = bitmap;
                            _isRendering = false;
                        }, DispatcherPriority.Render);
                    }
                }
                else
                {
                    // Handle raw BGRA frames (e.g., X11)
                    Dispatcher.UIThread.Post(() =>
                    {
                        try
                        {
                            if (_bitmap == null || _bitmap.Size.Width != frame.Width || _bitmap.Size.Height != frame.Height)
                            {
                                _bitmap = new WriteableBitmap(
                                    new PixelSize(frame.Width, frame.Height),
                                    new Vector(96, 96),
                                    PixelFormat.Bgra8888,
                                    AlphaFormat.Opaque);
                                VideoImage.Source = _bitmap;
                            }

                            using (var l = _bitmap.Lock())
                            {
                                Marshal.Copy(frame.Pixels, 0, l.Address, frame.Pixels.Length);
                            }
                            VideoImage.InvalidateVisual();
                        }
                        finally
                        {
                            _isRendering = false;
                        }
                    }, DispatcherPriority.Render);
                }
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[RemoteView] Decoding/Rendering error: {ex.Message}");
                _isRendering = false;
            }
        });
    }
    private double _lastX = -1;
    private double _lastY = -1;
    private DateTime _lastMoveTime = DateTime.MinValue;

    private void OnPointerMoved(object? sender, Avalonia.Input.PointerEventArgs e)
    {
        if (DataContext is MainWindowViewModel vm && vm.IsSessionActive)
        {
            // THROTTLING: Only send mouse moves every 30ms to avoid saturating the signaling hub
            if ((DateTime.Now - _lastMoveTime).TotalMilliseconds < 30) return;

            var pos = e.GetPosition(VideoImage);
            var x = pos.X / VideoImage.Bounds.Width;
            var y = pos.Y / VideoImage.Bounds.Height;

            if (x >= 0 && x <= 1 && y >= 0 && y <= 1)
            {
                // Prevent infinite mouse loop on localhost testing
                if (Math.Abs(x - _lastX) > 0.002 || Math.Abs(y - _lastY) > 0.002)
                {
                    _lastX = x;
                    _lastY = y;
                    _lastMoveTime = DateTime.Now;
                    _ = vm.SendInput(new NxDesk.Application.DTOs.InputEventDto(
                        NxDesk.Application.DTOs.InputEventType.MouseMove,
                        X: x,
                        Y: y
                    ));
                }
            }
        }
    }

    private void OnPointerPressed(object? sender, Avalonia.Input.PointerPressedEventArgs e)
    {
        Console.WriteLine($"[RemoteView] OnPointerPressed fired. DataContext Type: {DataContext?.GetType().Name}");
        
        // Focus the control to capture keyboard events
        this.Focus();
        
        if (DataContext is MainWindowViewModel vm)
        {
            Console.WriteLine($"[RemoteView] VM is present. IsSessionActive: {vm.IsSessionActive}");
            if (vm.IsSessionActive)
            {
                var point = e.GetCurrentPoint(VideoImage);
                var button = point.Properties.PointerUpdateKind switch
                {
                    Avalonia.Input.PointerUpdateKind.LeftButtonPressed => "Left",
                    Avalonia.Input.PointerUpdateKind.RightButtonPressed => "Right",
                    Avalonia.Input.PointerUpdateKind.MiddleButtonPressed => "Middle",
                    _ => "Left"
                };

                _ = vm.SendInput(new NxDesk.Application.DTOs.InputEventDto(
                    NxDesk.Application.DTOs.InputEventType.MouseDown,
                    MouseButton: button
                ));
            }
        }
    }
    private void OnPointerReleased(object? sender, Avalonia.Input.PointerReleasedEventArgs e)
    {
        if (DataContext is MainWindowViewModel vm && vm.IsSessionActive)
        {
            var point = e.GetCurrentPoint(VideoImage);
            var button = point.Properties.PointerUpdateKind switch
            {
                Avalonia.Input.PointerUpdateKind.LeftButtonReleased => "Left",
                Avalonia.Input.PointerUpdateKind.RightButtonReleased => "Right",
                Avalonia.Input.PointerUpdateKind.MiddleButtonReleased => "Middle",
                _ => "Left"
            };

            _ = vm.SendInput(new NxDesk.Application.DTOs.InputEventDto(
                NxDesk.Application.DTOs.InputEventType.MouseUp,
                MouseButton: button
            ));
        }
    }

    private void OnPointerWheelChanged(object? sender, Avalonia.Input.PointerWheelEventArgs e)
    {
        if (DataContext is MainWindowViewModel vm && vm.IsSessionActive)
        {
            _ = vm.SendInput(new NxDesk.Application.DTOs.InputEventDto(
                NxDesk.Application.DTOs.InputEventType.MouseWheel,
                DeltaX: (double)e.Delta.X,
                DeltaY: (double)e.Delta.Y
            ));
        }
    }

    private void OnKeyDown(object? sender, Avalonia.Input.KeyEventArgs e)
    {
        if (DataContext is MainWindowViewModel vm && vm.IsSessionActive)
        {
            _ = vm.SendInput(new NxDesk.Application.DTOs.InputEventDto(
                NxDesk.Application.DTOs.InputEventType.KeyDown,
                KeyCode: e.Key.ToString()
            ));
            
            // Mark as handled to prevent bubbling and potential infinite loops
            // if testing on localhost with xdotool
            e.Handled = true;
        }
    }
}
