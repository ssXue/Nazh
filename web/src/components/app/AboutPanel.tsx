import { useEffect, useState } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import nazhLogo from '../../assets/nazh-logo.svg';
import { Particles } from '../animations/Particles';

export function AboutPanel() {
  const [version, setVersion] = useState<string>('');

  useEffect(() => {
    getVersion().then(v => setVersion(v)).catch(() => {});
  }, []);

  return (
    <div className="about-screen">
        <Particles
          particleCount={320}
          particleSpread={8}
          speed={0.08}
          particleColors={['#ffffff', '#a0c4ff', '#bdb2ff']}
          moveParticlesOnHover
          particleHoverFactor={0.6}
          alphaParticles
          particleBaseSize={80}
          sizeRandomness={0.8}
          cameraDistance={22}
          className="about-screen__particles"
        />

        <div className="about-screen__center">
          <img className="about-screen__logo" src={nazhLogo} alt="Nazh logo" />
          {version && <span className="about-screen__version">Version {version}</span>}
        </div>
    </div>
  );
}
