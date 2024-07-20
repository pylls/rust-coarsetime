use benchmark_simple::*;
use coarsetime::*;
use maybenot::{
    action::Action,
    dist::{Dist, DistType},
    event::Event,
    state::{State, Trans},
    time::Duration,
    Framework, Machine, MachineId, TriggerEvent,
};
use std::{
    ops::{Add, AddAssign},
    time,
};

fn main() {
    let options = &Options {
        iterations: 250_000,
        warmup_iterations: 25_000,
        min_samples: 5,
        max_samples: 10,
        max_rsd: 1.0,
        ..Default::default()
    };
    bench_coarsetime_now(options);
    bench_coarsetime_recent(options);
    bench_coarsetime_elapsed(options);
    bench_coarsetime_elapsed_since_recent(options);
    bench_stdlib_now(options);
    bench_stdlib_elapsed(options);
    bench_maybenot_coarsetime(options);
    bench_maybenot_stdtime(options);
}

fn bench_coarsetime_now(options: &Options) {
    let b = Bench::new();
    Instant::update();
    let res = b.run(options, Instant::now);
    println!("coarsetime_now():          {}", res.throughput(1));
}

fn bench_coarsetime_recent(options: &Options) {
    let b = Bench::new();
    Instant::update();
    let res = b.run(options, Instant::recent);
    println!("coarsetime_recent():       {}", res.throughput(1));
}

fn bench_coarsetime_elapsed(options: &Options) {
    let b = Bench::new();
    let ts = Instant::now();
    let res = b.run(options, || ts.elapsed());
    println!("coarsetime_elapsed():      {}", res.throughput(1));
}

fn bench_coarsetime_elapsed_since_recent(options: &Options) {
    let b = Bench::new();
    let ts = Instant::now();
    let res = b.run(options, || ts.elapsed_since_recent());
    println!("coarsetime_since_recent(): {}", res.throughput(1));
}

fn bench_stdlib_now(options: &Options) {
    let b = Bench::new();
    let res = b.run(options, time::Instant::now);
    println!("stdlib_now():              {}", res.throughput(1));
}

fn bench_stdlib_elapsed(options: &Options) {
    let b = Bench::new();
    let ts = time::Instant::now();
    let res = b.run(options, || ts.elapsed());
    println!("stdlib_elapsed():          {}", res.throughput(1));
}

fn bench_maybenot_coarsetime(options: &Options) {
    let b = Bench::new();
    let res = b.run(options, run_maybenot_coarsetime);
    println!("maybenot_coarsetime():    {}", res.throughput(1));
}

fn bench_maybenot_stdtime(options: &Options) {
    let b = Bench::new();
    let res = b.run(options, run_maybenot_stdtime);
    println!("maybenot_stdtime():       {}", res.throughput(1));
}

#[derive(Copy, Clone, Debug)]
pub struct Inst(coarsetime::Instant);

impl maybenot::time::Instant for Inst {
    type Duration = Dur;

    fn saturating_duration_since(&self, earlier: Self) -> Self::Duration {
        Dur(self.0.duration_since(earlier.0))
    }
}

impl Inst {
    pub fn now() -> Self {
        Self(coarsetime::Instant::now())
    }
}

impl Add<Dur> for Inst {
    type Output = Inst;

    #[inline]
    fn add(self, rhs: Dur) -> Inst {
        Inst(self.0 + rhs.0)
    }
}

#[derive(Copy, Clone, PartialOrd, PartialEq, Debug)]
pub struct Dur(coarsetime::Duration);
impl maybenot::time::Duration for Dur {
    fn zero() -> Self {
        Dur(coarsetime::Duration::new(0, 0))
    }

    fn from_micros(micros: u64) -> Self {
        Dur(coarsetime::Duration::new(
            micros / 1_000_000,
            (micros % 1_000_000) as u32,
        ))
    }

    fn is_zero(&self) -> bool {
        self.0.as_f64() == 0.0
    }

    fn div_duration_f64(self, rhs: Self) -> f64 {
        self.0.as_f64() / rhs.0.as_f64()
    }
}

impl AddAssign for Dur {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

fn run_maybenot_coarsetime() {
    use enum_map::enum_map;

    // plan: create a machine that swaps between two states, trigger one
    // then multiple events and check the resulting actions

    // state 0: go to state 1 on PaddingSent, pad after 10 usec
    let mut s0 = State::new(enum_map! {
        Event::PaddingSent => vec![Trans(1, 1.0)],
    _ => vec![],
    });
    s0.action = Some(Action::SendPadding {
        bypass: false,
        replace: false,
        timeout: Dist {
            dist: DistType::Uniform {
                low: 10.0,
                high: 10.0,
            },
            start: 0.0,
            max: 0.0,
        },
        limit: None,
    });
    // state 1: go to state 0 on PaddingRecv, pad after 1 usec
    let mut s1 = State::new(enum_map! {
        Event::PaddingRecv => vec![Trans(0, 1.0)],
    _ => vec![],
    });
    s1.action = Some(Action::SendPadding {
        bypass: false,
        replace: false,
        timeout: Dist {
            dist: DistType::Uniform {
                low: 1.0,
                high: 1.0,
            },

            start: 0.0,
            max: 0.0,
        },
        limit: None,
    });

    // create a simple machine
    let m = Machine::new(1000, 1.0, 0, 0.0, vec![s0, s1]).unwrap();

    let mut current_time = Inst::now();
    let machines = vec![m];
    let mut f = Framework::new(&machines, 0.0, 0.0, current_time, rand::thread_rng()).unwrap();

    // start triggering
    assert_eq!(
        f.trigger_events(
            &[TriggerEvent::BlockingBegin {
                machine: MachineId::from_raw(0),
            }],
            current_time,
        )
        .into_iter()
        .count(),
        0
    );

    // move time forward, trigger again to make sure no scheduled timer
    current_time = current_time + (Dur::from_micros(20));
    assert_eq!(
        f.trigger_events(
            &[TriggerEvent::BlockingBegin {
                machine: MachineId::from_raw(0),
            }],
            current_time,
        )
        .count(),
        0
    );

    // trigger transition to next state
    assert_eq!(
        f.trigger_events(
            &[TriggerEvent::PaddingSent {
                machine: MachineId::from_raw(0),
            }],
            current_time,
        )
        .count(),
        1
    );

    // increase time, trigger event, make sure no further action
    current_time = current_time.add(Duration::from_micros(20));
    assert_eq!(
        f.trigger_events(
            &[TriggerEvent::PaddingSent {
                machine: MachineId::from_raw(0),
            }],
            current_time,
        )
        .count(),
        0
    );

    // go back to state 0
    assert_eq!(
        f.trigger_events(&[TriggerEvent::PaddingRecv], current_time)
            .count(),
        1
    );

    // test multiple triggers overwriting actions
    for _ in 0..10 {
        assert_eq!(
            f.trigger_events(
                &[
                    TriggerEvent::PaddingSent {
                        machine: MachineId::from_raw(0),
                    },
                    TriggerEvent::PaddingRecv,
                ],
                current_time,
            )
            .count(),
            1
        );
    }

    // triple trigger, swapping between states
    for i in 0..10 {
        if i % 2 == 0 {
            assert_eq!(
                f.trigger_events(
                    &[
                        TriggerEvent::PaddingRecv,
                        TriggerEvent::PaddingSent {
                            machine: MachineId::from_raw(0),
                        },
                        TriggerEvent::PaddingRecv,
                    ],
                    current_time,
                )
                .count(),
                1
            );
        } else {
            assert_eq!(
                f.trigger_events(
                    &[
                        TriggerEvent::PaddingSent {
                            machine: MachineId::from_raw(0),
                        },
                        TriggerEvent::PaddingRecv,
                        TriggerEvent::PaddingSent {
                            machine: MachineId::from_raw(0),
                        },
                    ],
                    current_time,
                )
                .count(),
                1
            );
        }
    }
}

fn run_maybenot_stdtime() {
    use enum_map::enum_map;

    // plan: create a machine that swaps between two states, trigger one
    // then multiple events and check the resulting actions

    // state 0: go to state 1 on PaddingSent, pad after 10 usec
    let mut s0 = State::new(enum_map! {
        Event::PaddingSent => vec![Trans(1, 1.0)],
    _ => vec![],
    });
    s0.action = Some(Action::SendPadding {
        bypass: false,
        replace: false,
        timeout: Dist {
            dist: DistType::Uniform {
                low: 10.0,
                high: 10.0,
            },
            start: 0.0,
            max: 0.0,
        },
        limit: None,
    });
    // state 1: go to state 0 on PaddingRecv, pad after 1 usec
    let mut s1 = State::new(enum_map! {
        Event::PaddingRecv => vec![Trans(0, 1.0)],
    _ => vec![],
    });
    s1.action = Some(Action::SendPadding {
        bypass: false,
        replace: false,
        timeout: Dist {
            dist: DistType::Uniform {
                low: 1.0,
                high: 1.0,
            },

            start: 0.0,
            max: 0.0,
        },
        limit: None,
    });

    // create a simple machine
    let m = Machine::new(1000, 1.0, 0, 0.0, vec![s0, s1]).unwrap();

    let mut current_time = time::Instant::now();
    let machines = vec![m];
    let mut f = Framework::new(&machines, 0.0, 0.0, current_time, rand::thread_rng()).unwrap();

    // start triggering
    assert_eq!(
        f.trigger_events(
            &[TriggerEvent::BlockingBegin {
                machine: MachineId::from_raw(0),
            }],
            current_time,
        )
        .into_iter()
        .count(),
        0
    );

    // move time forward, trigger again to make sure no scheduled timer
    current_time = current_time + (time::Duration::from_micros(20));
    assert_eq!(
        f.trigger_events(
            &[TriggerEvent::BlockingBegin {
                machine: MachineId::from_raw(0),
            }],
            current_time,
        )
        .count(),
        0
    );

    // trigger transition to next state
    assert_eq!(
        f.trigger_events(
            &[TriggerEvent::PaddingSent {
                machine: MachineId::from_raw(0),
            }],
            current_time,
        )
        .count(),
        1
    );

    // increase time, trigger event, make sure no further action
    current_time = current_time.add(Duration::from_micros(20));
    assert_eq!(
        f.trigger_events(
            &[TriggerEvent::PaddingSent {
                machine: MachineId::from_raw(0),
            }],
            current_time,
        )
        .count(),
        0
    );

    // go back to state 0
    assert_eq!(
        f.trigger_events(&[TriggerEvent::PaddingRecv], current_time)
            .count(),
        1
    );

    // test multiple triggers overwriting actions
    for _ in 0..10 {
        assert_eq!(
            f.trigger_events(
                &[
                    TriggerEvent::PaddingSent {
                        machine: MachineId::from_raw(0),
                    },
                    TriggerEvent::PaddingRecv,
                ],
                current_time,
            )
            .count(),
            1
        );
    }

    // triple trigger, swapping between states
    for i in 0..10 {
        if i % 2 == 0 {
            assert_eq!(
                f.trigger_events(
                    &[
                        TriggerEvent::PaddingRecv,
                        TriggerEvent::PaddingSent {
                            machine: MachineId::from_raw(0),
                        },
                        TriggerEvent::PaddingRecv,
                    ],
                    current_time,
                )
                .count(),
                1
            );
        } else {
            assert_eq!(
                f.trigger_events(
                    &[
                        TriggerEvent::PaddingSent {
                            machine: MachineId::from_raw(0),
                        },
                        TriggerEvent::PaddingRecv,
                        TriggerEvent::PaddingSent {
                            machine: MachineId::from_raw(0),
                        },
                    ],
                    current_time,
                )
                .count(),
                1
            );
        }
    }
}
