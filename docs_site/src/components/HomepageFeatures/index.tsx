import type {ReactNode} from 'react';
import clsx from 'clsx';
import Heading from '@theme/Heading';
import styles from './styles.module.css';

type FeatureItem = {
  title: string;
  description: ReactNode;
};

const FeatureList: FeatureItem[] = [
  {
    title: 'Orbital Mechanics Simulation',
    description: (
      <>
        Keplerian orbit propagation with TLE and classical orbital element (COE)
        support, powered by the{' '}
        <a href="https://github.com/duncaneddy/brahe">brahe</a> library.
      </>
    ),
  },
  {
    title: 'Space Tether Dynamics',
    description: (
      <>
        Physics-based multi-joint tether modelling using{' '}
        <a href="https://github.com/Jondolf/avian">Avian3D</a> rigid-body
        simulation, with configurable joint count and spacing.
      </>
    ),
  },
  {
    title: '3D Bevy Visualization',
    description: (
      <>
        Real-time 3D Earth rendering with atmospheric scattering, a dual render
        layer scene/map view, and an interactive orbit camera.
      </>
    ),
  },
];

function Feature({title, description}: FeatureItem) {
  return (
    <div className={clsx('col col--4')}>
      <div className="text--center padding-horiz--md">
        <Heading as="h3">{title}</Heading>
        <p>{description}</p>
      </div>
    </div>
  );
}

export default function HomepageFeatures(): ReactNode {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}
