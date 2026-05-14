import nazhLogo from '../../assets/nazh-logo.svg';
import { Particles } from '../animations/Particles';

const ABOUT_VERSION = 'Version 1.0.0';

export function AboutPanel() {
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
          <span className="about-screen__version">{ABOUT_VERSION}</span>
        </div>
    </div>
  );
}
