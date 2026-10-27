using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Data.Core.Plugins;
using Avalonia.Markup.Xaml;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using NxDesk.Application.Interfaces;
using NxDesk.Client.ViewModels;
using NxDesk.Client.Views;
using NxDesk.Client.Services;
using NxDesk.Infrastructure.Services;

namespace NxDesk.Client;

public partial class App : Avalonia.Application
{
    public override void Initialize()
    {
        AvaloniaXamlLoader.Load(this);
    }

    public override void OnFrameworkInitializationCompleted()
    {
        // Setup DI
        var services = new ServiceCollection();
        string[] args = Array.Empty<string>();
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktopLifetime)
        {
            args = desktopLifetime.Args ?? Array.Empty<string>();
        }
        ConfigureServices(services, args);
        var serviceProvider = services.BuildServiceProvider();

        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            // Avoid duplicate resolutions in design time or other scenarios
            BindingPlugins.DataValidators.RemoveAt(0);
            
            var config = serviceProvider.GetRequiredService<IConfigurationService>();
            var themeService = serviceProvider.GetRequiredService<IThemeService>();

            ApplyFontSize(config.FontSize);
            ApplyLanguage(config.Language);
            themeService.SetTheme(config.Theme);
            
            config.FontSizeChanged += ApplyFontSize;
            config.LanguageChanged += ApplyLanguage;
            config.ThemeChanged += theme => themeService.SetTheme(theme);

            desktop.MainWindow = new MainWindow(serviceProvider)
            {
                DataContext = serviceProvider.GetRequiredService<MainWindowViewModel>()
            };
        }

        base.OnFrameworkInitializationCompleted();
    }

    private void TrayIcon_OnClicked(object? sender, EventArgs e)
    {
        ToggleWindow();
    }

    private void ToggleWindow_Click(object? sender, EventArgs e)
    {
        ToggleWindow();
    }

    private void ToggleWindow()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop && desktop.MainWindow != null)
        {
            if (desktop.MainWindow.IsVisible)
            {
                desktop.MainWindow.Hide();
            }
            else
            {
                desktop.MainWindow.Show();
                desktop.MainWindow.Activate();
            }
        }
    }

    private void Exit_Click(object? sender, EventArgs e)
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            // Set a flag or handle shutdown properly to avoid being intercepted by MainWindow.Closing
            if (desktop.MainWindow is MainWindow mainWin)
            {
                mainWin.IsExiting = true;
            }
            desktop.Shutdown();
        }
    }

    private void ApplyFontSize(AppFontSize size)
    {
        double scale = size switch
        {
            AppFontSize.Small => 12,
            AppFontSize.Normal => 14,
            AppFontSize.Large => 16,
            _ => 14
        };
        Resources["AppFontSize"] = scale;
        Resources["FontSizeTitle"] = scale + 10;
    }

    private void ApplyLanguage(AppLanguage language)
    {
        string culture = language == AppLanguage.Spanish ? "es-ES" : "en-US";
        var uri = new Uri($"avares://NxDesk.Client/Assets/Languages/{culture}.axaml");
        
        try 
        {
            var dict = (ResourceDictionary)Avalonia.Markup.Xaml.AvaloniaXamlLoader.Load(uri);
            
            // Find and replace existing language dictionary if any
            var existing = Resources.MergedDictionaries.FirstOrDefault(d => d is ResourceDictionary rd && rd.ContainsKey("S_Settings_Title"));
            if (existing != null)
            {
                Resources.MergedDictionaries.Remove(existing);
            }
            Resources.MergedDictionaries.Add(dict);
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Error loading language {culture}: {ex.Message}");
        }
    }

    private void ConfigureServices(IServiceCollection services, string[] args)
    {
        services.AddSingleton(args);
        // Configuration
        var configBuilder = new ConfigurationBuilder();
        configBuilder.AddInMemoryCollection(new Dictionary<string, string?>
        {
            ["SignalR:ServerUrl"] = "http://localhost:5000/signalinghub"
        });
        var configuration = configBuilder.Build();
        services.AddSingleton<IConfiguration>(configuration);

        // Services
        services.AddSingleton<IDiscoveryService, DiscoveryService>();
        services.AddSingleton<ISignalingService, SignalRSignalingService>();
        
        // Cross-platform screen capture
        if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
        {
            services.AddSingleton<IScreenCaptureService, DesktopDuplicationCaptureService>();
        }
        else if (RuntimeInformation.IsOSPlatform(OSPlatform.Linux))
        {
            // Use GStreamer (PipeWire) for Wayland support as primary backend
            services.AddSingleton<IScreenCaptureService, GstreamerCaptureService>();
        }
        else
        {
            // Placeholder/Null for unsupported platforms
            services.AddSingleton<IScreenCaptureService, DesktopDuplicationCaptureService>(); 
        }

        services.AddSingleton<IConfigurationService, ConfigurationService>();
        services.AddSingleton<IThemeService, ThemeService>();

        // Input Injection (Remote Control) — platform-specific
        if (RuntimeInformation.IsOSPlatform(OSPlatform.Linux))
        {
            services.AddSingleton<IInputInjectionService, LinuxInputInjectionService>();
        }
        else if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
        {
            services.AddSingleton<IInputInjectionService, WindowsInputInjectionService>();
        }
        else
        {
            // macOS or other — fallback to a no-op stub (could be replaced later)
            services.AddSingleton<IInputInjectionService, LinuxInputInjectionService>();
        }

        // Remote Terminal
        services.AddSingleton<IRemoteTerminalService, RemoteTerminalService>();

        // File Transfer
        services.AddSingleton<IFileTransferService, FileTransferService>();

        // Screen Recording
        services.AddSingleton<IScreenRecordingService, ScreenRecordingService>();

        // ViewModels
        services.AddTransient<MainWindowViewModel>();
        services.AddTransient<SettingsViewModel>();
    }
}