# iRacing Companion Launcher - Refactored

A well-structured Python application following best practices for code organization and maintainability.

## Project Structure

```
iracing_launcher_app/
├── __init__.py           # Package initialization
├── constants.py          # Application constants and configuration
├── config_manager.py     # Config file reading/writing
├── app_manager.py        # Application path detection and process management
├── widgets.py            # Custom UI widgets (ModernButton, StatusCard)
└── gui.py               # Main GUI application class

iracing_launcher.py    # Entry point script
```

## Module Overview

### `constants.py`
Centralized configuration values:
- UI color schemes (dark theme)
- Status colors
- Widget dimensions
- Application definitions with paths and shortcuts

### `config_manager.py`
**Class: `ConfigManager`**
- Manages `config.ini` file for persistent app paths
- Methods:
  - `get_app_path(app_key)` - Retrieve saved path
  - `set_app_path(app_key, path)` - Save app path
  - `get_config_key(app_name)` - Convert app name to config key

### `app_manager.py`
**Class: `AppManager`**
- Handles application detection and process operations
- Methods:
  - `find_app_path(app_name)` - Auto-detect or retrieve app path
  - `launch_app(app_name, app_path)` - Launch and verify app
  - `close_app(app_name)` - Kill app process
  - `is_process_running(process_name)` - Check if process is running
  - `get_app_list()` - Get all managed app names

### `widgets.py`
**Class: `ModernButton`**
- Custom Canvas-based rounded button with hover effects
- State management (enabled/disabled)

**Class: `StatusCard`**
- Frame widget displaying app status
- Shows colored status indicator or Browse button
- States: idle, starting, running, failed, stopped, not_found

### `gui.py`
**Class: `iRacingLauncherGUI`**
- Main application GUI class
- Coordinates between config manager and app manager
- Methods:
  - `launch_apps()` - Launch all configured apps
  - `close_apps()` - Close all running apps
  - `_browse_for_app(app_name)` - Manual path selection dialog
  - `_log_message(message, level)` - Activity logging

## Design Principles Applied

### 1. Separation of Concerns
- UI logic (gui.py) separated from business logic (app_manager.py)
- Configuration management isolated (config_manager.py)
- Constants extracted to separate module

### 2. Single Responsibility Principle
- Each class has one clear purpose
- ConfigManager only handles config files
- AppManager only handles app detection and processes
- GUI only handles user interface

### 3. Dependency Injection
- GUI receives manager instances instead of creating them
- Makes testing easier and reduces coupling

### 4. Type Hints
- All functions include type hints for parameters and returns
- Improves IDE support and code clarity

### 5. Documentation
- Comprehensive docstrings for all classes and methods
- Clear parameter and return value descriptions

### 6. Error Handling
- Graceful error handling with try/except blocks
- User-friendly error messages in the UI

## Usage

Run the application:
```bash
python iracing_launcher.py
```

## Migration from Old Version

The old monolithic `iracing_launcher.py` has been refactored into the modular structure above. All functionality remains the same:
- Auto-detection of installed apps
- Manual path configuration via Browse button
- Batch launch/close operations
- Activity logging
- config.ini persistence

## Benefits of Refactoring

1. **Maintainability**: Easier to find and fix bugs
2. **Testability**: Each module can be tested independently
3. **Readability**: Smaller, focused files are easier to understand
4. **Extensibility**: Easy to add new features without affecting other parts
5. **Reusability**: Modules can be reused in other projects
6. **IDE Support**: Better autocomplete and type checking

## Future Enhancements

With this structure, it's now easier to add:
- Unit tests for each module
- Command-line interface
- Plugin system for additional apps
- Settings dialog for customization
- Logging to file
- Update checker
