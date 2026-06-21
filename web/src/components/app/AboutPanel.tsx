import { useEffect, useState } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import nazhLogo from '../../assets/nazh-logo.svg';

export function AboutPanel() {
  const [version, setVersion] = useState<string>('');

  useEffect(() => {
    getVersion().then(v => setVersion(v)).catch(() => {});
  }, []);

  return (
    <div className="about-screen">
        <div className="about-screen__center">
          <img className="about-screen__logo" src={nazhLogo} alt="Nazh logo" />
          {version && <span className="about-screen__version">Version {version}</span>}
        </div>
    </div>
  );
}
