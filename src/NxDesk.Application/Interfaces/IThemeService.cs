namespace NxDesk.Application.Interfaces;

public enum AppTheme
{
    Light,
    Dark
}

public interface IThemeService
{
    AppTheme CurrentTheme { get; }
    void SetTheme(AppTheme theme);
    void ToggleTheme();
}
