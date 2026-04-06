import { useEffect, useState } from 'react';

import {
  MoonIcon,
  SunIcon,
  WindowCloseIcon,
  WindowMaximizeIcon,
  WindowMinimizeIcon,
  WindowRestoreIcon,
} from './AppIcons';
import nazhLogo from '../../assets/nazh-logo.svg';
import {
  closeCurrentWindow,
  minimizeCurrentWindow,
  toggleCurrentWindowMaximize,
  watchCurrentWindowMaximized,
} from '../../lib/tauri';
import type { StudioTitleBarProps } from './types';
import type { MouseEvent } from 'react';

export function StudioTitleBar({
  isTauriRuntime,
  runtimeModeLabel,
  workflowStatusLabel,
  workflowStatusPillClass,
  themeMode,
  onToggleTheme,
}: StudioTitleBarProps) {
  const isDarkMode = themeMode === 'dark';
  const [isWindowMaximized, setIsWindowMaximized] = useState(false);

  function handleWindowControlPointer(event: MouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();
  }

  function createWindowActionHandler(action: () => Promise<void>) {
    return (event: MouseEvent<HTMLButtonElement>) => {
      handleWindowControlPointer(event);
      void action();
    };
  }

  useEffect(() => {
    if (!isTauriRuntime) {
      setIsWindowMaximized(false);
      return;
    }

    let cleanup = () => {};
    let active = true;

    void watchCurrentWindowMaximized((nextState) => {
      if (active) {
        setIsWindowMaximized(nextState);
      }
    }).then((nextCleanup) => {
      if (active) {
        cleanup = nextCleanup;
      } else {
        nextCleanup();
      }
    });

    return () => {
      active = false;
      cleanup();
    };
  }, [isTauriRuntime]);

  return (
    <header className="studio-titlebar">
      {isTauriRuntime ? (
        <section className="studio-window-controls" aria-label="窗体控制">
          <button
            type="button"
            className="studio-window-control studio-window-control--close"
            aria-label="关闭窗口"
            title="关闭窗口"
            onMouseDown={handleWindowControlPointer}
            onDoubleClick={handleWindowControlPointer}
            onClick={createWindowActionHandler(closeCurrentWindow)}
          >
            <WindowCloseIcon />
          </button>
          <button
            type="button"
            className="studio-window-control studio-window-control--minimize"
            aria-label="最小化窗口"
            title="最小化窗口"
            onMouseDown={handleWindowControlPointer}
            onDoubleClick={handleWindowControlPointer}
            onClick={createWindowActionHandler(minimizeCurrentWindow)}
          >
            <WindowMinimizeIcon />
          </button>
          <button
            type="button"
            className="studio-window-control studio-window-control--maximize"
            aria-label={isWindowMaximized ? '还原窗口' : '最大化窗口'}
            title={isWindowMaximized ? '还原窗口' : '最大化窗口'}
            onMouseDown={handleWindowControlPointer}
            onDoubleClick={handleWindowControlPointer}
            onClick={createWindowActionHandler(toggleCurrentWindowMaximize)}
          >
            {isWindowMaximized ? <WindowRestoreIcon /> : <WindowMaximizeIcon />}
          </button>
        </section>
      ) : null}
      <section className="studio-titlebar__brand" data-tauri-drag-region>
        <img className="studio-titlebar__logo" src={nazhLogo} alt="Nazh logo" />
        <div>
          <h1>Nazh</h1>
        </div>
      </section>
      <div className="studio-titlebar__drag" aria-hidden="true" data-tauri-drag-region />
      <section className="studio-titlebar__meta">
        <button
          type="button"
          className="studio-theme-toggle"
          aria-label={isDarkMode ? '切换到亮色主题' : '切换到暗色主题'}
          onClick={onToggleTheme}
        >
          <span className="studio-theme-toggle__icon" aria-hidden="true">
            {isDarkMode ? <SunIcon /> : <MoonIcon />}
          </span>
          <span>{isDarkMode ? '切换亮色' : '切换暗色'}</span>
        </button>
        <span className={`hero__runtime ${isTauriRuntime ? 'is-live' : 'is-preview'}`}>
          {runtimeModeLabel}
        </span>
        <span className={`runtime-pill ${workflowStatusPillClass}`}>{workflowStatusLabel}</span>
      </section>
    </header>
  );
}
