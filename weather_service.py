"""Artume Weather — Voice-driven weather forecasts."""

import json
import os
import urllib.request
import urllib.parse
from typing import Optional


class WeatherService:
    """Voice-driven weather information."""

    def __init__(self):
        self._cache = {}
        self._api_key = os.environ.get("OPENWEATHER_API_KEY", "")

    def get_weather(self, location: Optional[str] = None) -> str:
        """Get current weather for a location."""
        if not self._api_key:
            return ("Weather service requires an API key. "
                    "Set the OPENWEATHER_API_KEY environment variable, "
                    "or say 'weather for <city>' and I'll use a free source.")

        try:
            if not location:
                # Try to guess from IP
                location = self._get_location_from_ip()

            url = (f"https://api.openweathermap.org/data/2.5/weather?"
                   f"q={urllib.parse.quote(location)}&units=metric&appid={self._api_key}")
            req = urllib.request.Request(url, headers={"User-Agent": "ArtumeOS/1.0"})
            with urllib.request.urlopen(req, timeout=10) as resp:
                data = json.loads(resp.read())

            temp = round(data["main"]["temp"])
            feels_like = round(data["main"]["feels_like"])
            humidity = data["main"]["humidity"]
            description = data["weather"][0]["description"]
            wind_speed = round(data["wind"]["speed"])
            city = data["name"]

            return (f"Weather in {city}: {description}. "
                    f"Temperature {temp} degrees, feels like {feels_like}. "
                    f"Humidity {humidity} percent. Wind {wind_speed} meters per second.")
        except Exception as e:
            return f"Failed to get weather: {str(e)[:60]}"

    def get_forecast(self, location: Optional[str] = None) -> str:
        """Get 5-day weather forecast."""
        if not self._api_key:
            return "Weather forecast requires an API key."

        try:
            if not location:
                location = self._get_location_from_ip()

            url = (f"https://api.openweathermap.org/data/2.5/forecast?"
                   f"q={urllib.parse.quote(location)}&units=metric&cnt=5&appid={self._api_key}")
            req = urllib.request.Request(url, headers={"User-Agent": "ArtumeOS/1.0"})
            with urllib.request.urlopen(req, timeout=10) as resp:
                data = json.loads(resp.read())

            city = data["city"]["name"]
            forecasts = []
            for item in data["list"]:
                date = item["dt_txt"][:10]
                temp = round(item["main"]["temp"])
                desc = item["weather"][0]["description"]
                forecasts.append(f"{date}: {desc}, {temp} degrees")

            return f"5-day forecast for {city}: " + ". ".join(forecasts[:5]) + "."
        except Exception as e:
            return f"Failed to get forecast: {str(e)[:60]}"

    def _get_location_from_ip(self) -> str:
        """Get approximate location from IP address."""
        try:
            req = urllib.request.Request("http://ip-api.com/json/", headers={"User-Agent": "ArtumeOS/1.0"})
            with urllib.request.urlopen(req, timeout=5) as resp:
                data = json.loads(resp.read())
            return f"{data.get('city', 'London')},{data.get('countryCode', 'GB')}"
        except Exception:
            return "London"


# Global singleton
_weather: Optional[WeatherService] = None


def get_weather() -> WeatherService:
    global _weather
    if _weather is None:
        _weather = WeatherService()
    return _weather
