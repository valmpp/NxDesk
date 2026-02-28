using Avalonia;
using Avalonia.Markup.Xaml.Styling;
using NxDesk.Application.Interfaces;
using System;
using System.Linq;

namespace NxDesk.Client.Services;

public class ThemeService : IThemeService
{
    private AppTheme _currentTheme = AppTheme.Dark;
    public AppTheme CurrentTheme => _currentTheme;

    public void SetTheme(AppTheme theme)
    {
        _currentTheme = theme;
        var app = Avalonia.Application.Current;
        if (app == null) return;

        app.RequestedThemeVariant = theme == AppTheme.Dark 
            ? Avalonia.Styling.ThemeVariant.Dark 
            : Avalonia.Styling.ThemeVariant.Light;
    }

    public void ToggleTheme()
    {
        SetTheme(_currentTheme == AppTheme.Dark ? AppTheme.Light : AppTheme.Dark);
    }
}
