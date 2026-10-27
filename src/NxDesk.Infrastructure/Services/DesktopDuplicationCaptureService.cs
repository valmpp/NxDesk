using SharpDX;
using SharpDX.Direct3D11;
using SharpDX.DXGI;
using System.Runtime.InteropServices;
using NxDesk.Application.DTOs;
using NxDesk.Application.Interfaces;
using Device = SharpDX.Direct3D11.Device;
using MapFlags = SharpDX.Direct3D11.MapFlags;

namespace NxDesk.Infrastructure.Services;

/// <summary>
/// Desktop Duplication API (DXGI) based screen capture service.
/// Provides GPU-accelerated screen capture for Windows.
/// Much more efficient than GDI BitBlt - uses GPU directly.
/// </summary>
public class DesktopDuplicationCaptureService : IScreenCaptureService
{
    private Device? _device;
    private OutputDuplication? _duplicatedOutput;
    private Texture2D? _screenTexture;
    private int _width;
    private int _height;
    private bool _initialized = false;
    private int _currentAdapterIndex = 0;
    private int _currentOutputIndex = 0;

    // Reusable buffer to avoid allocations
    private byte[]? _frameBuffer;

    public int Width => _width;
    public int Height => _height;
    public bool IsInitialized => _initialized;

    /// <summary>
    /// Initialize capture for a specific monitor
    /// </summary>
    /// <param name="monitorIndex">Monitor index (0 = primary)</param>
    /// <returns>True if initialization successful</returns>
    public bool Initialize(int x = 0, int y = 0, int width = 0, int height = 0, int monitorIndex = 0)
    {
        try
        {
            Dispose(); // Clean up previous resources

            using var factory = new Factory1();

            // Find the correct adapter and output
            int outputCount = 0;
            for (int adapterIndex = 0; adapterIndex < factory.GetAdapterCount1(); adapterIndex++)
            {
                using var adapter = factory.GetAdapter1(adapterIndex);

                for (int outputIndex = 0; outputIndex < adapter.GetOutputCount(); outputIndex++)
                {
                    if (outputCount == monitorIndex)
                    {
                        _currentAdapterIndex = adapterIndex;
                        _currentOutputIndex = outputIndex;
                        break;
                    }
                    outputCount++;
                }
            }

            // Create device with selected adapter
            using var selectedAdapter = factory.GetAdapter1(_currentAdapterIndex);
            _device = new Device(selectedAdapter, DeviceCreationFlags.BgraSupport);

            using var output = selectedAdapter.GetOutput(_currentOutputIndex);
            using var output1 = output.QueryInterface<Output1>();

            var bounds = output.Description.DesktopBounds;
            _width = bounds.Right - bounds.Left;
            _height = bounds.Bottom - bounds.Top;

            // Ensure even dimensions for video codecs
            if (_width % 2 != 0) _width--;
            if (_height % 2 != 0) _height--;

            // Create staging texture for reading pixels
            var textureDesc = new Texture2DDescription
            {
                CpuAccessFlags = CpuAccessFlags.Read,
                BindFlags = BindFlags.None,
                Format = Format.B8G8R8A8_UNorm,
                Width = _width,
                Height = _height,
                OptionFlags = ResourceOptionFlags.None,
                MipLevels = 1,
                ArraySize = 1,
                SampleDescription = { Count = 1, Quality = 0 },
                Usage = ResourceUsage.Staging
            };
            _screenTexture = new Texture2D(_device, textureDesc);

            // Duplicate output
            _duplicatedOutput = output1.DuplicateOutput(_device);

            // Pre-allocate buffer
            _frameBuffer = new byte[_width * _height * 4];

            _initialized = true;
            Console.WriteLine($"[DXGI] Initialized: {_width}x{_height}, Monitor {monitorIndex}");
            return true;
        }
        catch (SharpDXException ex) when (ex.ResultCode.Code == SharpDX.DXGI.ResultCode.NotCurrentlyAvailable.Code)
        {
            Console.WriteLine("[DXGI] Error: Desktop Duplication not available (another app using it?)");
            return false;
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[DXGI] Initialization error: {ex.Message}");
            return false;
        }
    }

    /// <summary>
    /// Capture a frame. Returns null if no changes or error.
    /// </summary>
    /// <param name="timeoutMs">Timeout in milliseconds</param>
    public VideoFrameDto? CaptureFrame(int timeoutMs = 100)
    {
        if (!_initialized || _duplicatedOutput == null || _device == null || _screenTexture == null || _frameBuffer == null)
            return null;

        SharpDX.DXGI.Resource? screenResource = null;

        try
        {
            // Try to acquire next frame
            var result = _duplicatedOutput.TryAcquireNextFrame(timeoutMs, out var frameInfo, out screenResource);

            if (result.Failure)
            {
                // Timeout or error - no new frame
                return null;
            }

            // If no changes on screen, skip
            if (frameInfo.TotalMetadataBufferSize == 0 && frameInfo.LastPresentTime == 0)
            {
                _duplicatedOutput.ReleaseFrame();
                return null;
            }

            using var screenTexture2D = screenResource.QueryInterface<Texture2D>();

            // Copy to our staging texture
            _device.ImmediateContext.CopyResource(screenTexture2D, _screenTexture);

            // Map texture to read pixels
            var dataBox = _device.ImmediateContext.MapSubresource(
                _screenTexture,
                0,
                MapMode.Read,
                MapFlags.None);

            try
            {
                var sourcePtr = dataBox.DataPointer;
                int rowPitch = dataBox.RowPitch;

                // Copy row by row (in case of padding)
                for (int y = 0; y < _height; y++)
                {
                    Marshal.Copy(
                        sourcePtr + y * rowPitch,
                        _frameBuffer,
                        y * _width * 4,
                        _width * 4);
                }

                return new VideoFrameDto(_frameBuffer, _width, _height);
            }
            finally
            {
                _device.ImmediateContext.UnmapSubresource(_screenTexture, 0);
            }
        }
        catch (SharpDXException ex) when (ex.ResultCode.Code == SharpDX.DXGI.ResultCode.AccessLost.Code)
        {
            Console.WriteLine("[DXGI] Access lost, reinitializing...");
            _initialized = false;
            return null;
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[DXGI] Capture error: {ex.Message}");
            return null;
        }
        finally
        {
            try
            {
                screenResource?.Dispose();
                _duplicatedOutput?.ReleaseFrame();
            }
            catch { }
        }
    }

    public void Dispose()
    {
        _initialized = false;

        try { _duplicatedOutput?.Dispose(); } catch { }
        try { _screenTexture?.Dispose(); } catch { }
        try { _device?.Dispose(); } catch { }

        _duplicatedOutput = null;
        _screenTexture = null;
        _device = null;
    }
}
