using Avalonia.Data.Converters;
using System;
using System.Globalization;

namespace NxDesk.Client.Converters;

public class EqualsIntConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is int val && parameter is string paramStr && int.TryParse(paramStr, out int paramVal))
        {
            return val == paramVal;
        }
        return false;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}
