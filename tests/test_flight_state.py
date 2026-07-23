"""Tests for the flight state machine.

This is the logic that gets exercised exactly once per flight, irreversibly,
300 m up. It is also the only part of the firmware that is pure computation,
so it is both the most valuable and the easiest thing to test.

The approach: synthesize a pressure and acceleration profile for a plausible
D12-5 flight, feed it through FlightState at the real sample rate, and assert
the state sequence and the transition timings.
"""

import math

import pytest

from rocket.code import (
    APOGEE_VEL_MPS,
    BOOST_MIN_MS,
    BOOST_THRESHOLD_G,
    COAST_THRESHOLD_G,
    LANDED_HOLD_MS,
    FlightState,
    accel_magnitude,
)
from common.packet import State

GROUND_HPA = 1013.25
BARO_DT_MS = 40  # 25 Hz


def alt_to_pressure(alt_m, ground_hpa=GROUND_HPA):
    """Inverse of FlightState._pressure_to_alt."""
    return ground_hpa * math.pow(1.0 - alt_m / 44330.0, 1.0 / 0.1903)


# --- The altitude model ------------------------------------------------------


def d12_profile(t):
    """Altitude (m) and total acceleration (g) at time t seconds.

    Loosely a D12-5 lofting ~190 g: 1.6 s burn to ~90 m, coast to ~300 m at
    about 7 s, ejection, then ~5 m/s under the chute. Numbers are illustrative,
    not a simulation -- the point is the SHAPE, which is what the state machine
    keys on.
    """
    burn = 1.6
    apogee_t = 7.0

    if t < burn:
        a_g = 6.5
        alt = 0.5 * (a_g * 9.80665 - 9.80665) * t * t
    elif t < apogee_t:
        v0 = (6.5 * 9.80665 - 9.80665) * burn
        alt0 = 0.5 * (6.5 * 9.80665 - 9.80665) * burn * burn
        dt = t - burn
        alt = alt0 + v0 * dt - 0.5 * 9.80665 * dt * dt
        a_g = 1.0  # coasting: only gravity
    else:
        v0 = (6.5 * 9.80665 - 9.80665) * burn
        alt0 = 0.5 * (6.5 * 9.80665 - 9.80665) * burn * burn
        dtc = apogee_t - burn
        peak = alt0 + v0 * dtc - 0.5 * 9.80665 * dtc * dtc
        alt = peak - 5.0 * (t - apogee_t)  # 5 m/s descent
        a_g = 1.0

    return max(0.0, alt), a_g


def run_profile(fs, duration_s, dt_ms=BARO_DT_MS, profile=d12_profile,
                start_state=State.ARMED):
    """Drive FlightState through a profile. Returns [(t_ms, state), ...]."""
    fs.set_ground_reference(GROUND_HPA)
    fs.transition(start_state, 0)

    history = [(0, fs.state)]
    steps = int(duration_s * 1000 / dt_ms)

    for i in range(1, steps + 1):
        t_ms = i * dt_ms
        alt, a_g = profile(t_ms / 1000.0)
        fs.update_altitude(alt_to_pressure(alt), t_ms)
        if fs.update(a_g, t_ms):
            history.append((t_ms, fs.state))

    return history


# --- Altitude and velocity ---------------------------------------------------


def test_pressure_to_altitude_is_invertible():
    fs = FlightState()
    fs.set_ground_reference(GROUND_HPA)
    for alt in (0, 10, 100, 300, 1000):
        assert abs(fs._pressure_to_alt(alt_to_pressure(alt)) - alt) < 0.5


def test_altitude_is_zero_without_ground_reference():
    """Before ARM there is no datum. Reporting a real altitude would be a lie."""
    fs = FlightState()
    assert fs._pressure_to_alt(900.0) == 0.0


def test_ground_reference_makes_pad_altitude_zero():
    fs = FlightState()
    fs.set_ground_reference(987.6)  # not sea level -- Omaha is ~330 m
    assert abs(fs._pressure_to_alt(987.6)) < 0.01


def test_velocity_converges_on_a_steady_climb():
    """EMA has lag by design; after enough samples it must track truth."""
    fs = FlightState()
    fs.set_ground_reference(GROUND_HPA)
    for i in range(200):
        t = i * BARO_DT_MS
        fs.update_altitude(alt_to_pressure(10.0 * t / 1000.0), t)
    assert abs(fs.vel_mps - 10.0) < 0.5


def test_velocity_is_negative_while_descending():
    fs = FlightState()
    fs.set_ground_reference(GROUND_HPA)
    for i in range(200):
        t = i * BARO_DT_MS
        fs.update_altitude(alt_to_pressure(300.0 - 5.0 * t / 1000.0), t)
    assert fs.vel_mps < -4.0


def test_max_altitude_is_latched():
    fs = FlightState()
    run_profile(fs, 120.0)
    assert fs.max_alt_m > fs.alt_m


# --- Full flight -------------------------------------------------------------


def test_full_flight_visits_every_state_in_order():
    fs = FlightState()
    history = run_profile(fs, 120.0)
    states = [s for _, s in history]
    assert states == [
        State.ARMED,
        State.BOOST,
        State.COAST,
        State.APOGEE,
        State.DESCENT,
        State.LANDED,
    ]


def test_boost_fires_shortly_after_ignition():
    fs = FlightState()
    history = dict((s, t) for t, s in run_profile(fs, 120.0))
    # Must wait out BOOST_MIN_MS, but not much longer.
    assert BOOST_MIN_MS <= history[State.BOOST] <= BOOST_MIN_MS + 200


def test_burnout_detected_near_end_of_burn():
    fs = FlightState()
    history = dict((s, t) for t, s in run_profile(fs, 120.0))
    assert 1500 <= history[State.COAST] <= 1900  # 1.6 s burn


def test_apogee_detected_near_the_actual_peak():
    fs = FlightState()
    history = dict((s, t) for t, s in run_profile(fs, 120.0))
    assert 6300 <= history[State.APOGEE] <= 7600  # true apogee ~7.0 s


def test_landed_requires_the_hold_period():
    fs = FlightState()
    history = dict((s, t) for t, s in run_profile(fs, 120.0))
    assert history[State.LANDED] - history[State.DESCENT] >= LANDED_HOLD_MS


def test_apogee_altitude_is_plausible():
    fs = FlightState()
    run_profile(fs, 120.0)
    assert 200 < fs.max_alt_m < 400


# --- Transitions are one-way -------------------------------------------------


def test_no_path_back_to_armed_after_boost():
    """A rocket that has left the pad must not re-arm mid-flight."""
    fs = FlightState()
    history = run_profile(fs, 120.0)
    seen = [s for _, s in history]
    assert seen.index(State.BOOST) < seen.index(State.COAST)
    assert State.ARMED not in seen[1:]


def test_state_never_regresses():
    fs = FlightState()
    history = run_profile(fs, 120.0)
    values = [s for _, s in history]
    assert values == sorted(values)


# --- Rejection of false triggers ---------------------------------------------


def test_a_brief_bump_does_not_trigger_boost():
    """Handling the rocket on the pad must not launch the state machine."""
    fs = FlightState()

    def bump(t):
        # 5 g spike lasting 80 ms -- shorter than BOOST_MIN_MS
        a = 8.0 if 1.0 <= t < 1.08 else 1.0
        return 0.0, a

    history = run_profile(fs, 5.0, dt_ms=10, profile=bump)
    assert [s for _, s in history] == [State.ARMED]


def test_a_sustained_bump_does_trigger_boost():
    """The rejection above must not be so aggressive it misses a real launch."""
    fs = FlightState()

    def sustained(t):
        a = 8.0 if t >= 1.0 else 1.0
        alt = 0.0 if t < 1.0 else 30.0 * (t - 1.0) ** 2
        return alt, a

    history = run_profile(fs, 5.0, dt_ms=10, profile=sustained)
    assert State.BOOST in [s for _, s in history]


def test_idle_does_not_advance_on_acceleration():
    """Only an uplink ARM leaves IDLE. Shaking the payload must do nothing."""
    fs = FlightState()
    history = run_profile(fs, 5.0, start_state=State.IDLE)
    assert [s for _, s in history] == [State.IDLE]


def test_boot_does_not_advance_on_acceleration():
    fs = FlightState()
    history = run_profile(fs, 5.0, start_state=State.BOOT)
    assert [s for _, s in history] == [State.BOOT]


# --- Thresholds are self-consistent ------------------------------------------


def test_coast_threshold_below_boost_threshold():
    """Otherwise BOOST and COAST could both be true, or neither."""
    assert COAST_THRESHOLD_G < BOOST_THRESHOLD_G


def test_boost_threshold_above_one_g():
    """A rocket sitting on the pad reads 1 g. Anything at or below that
    would fire BOOST the instant it is armed."""
    assert BOOST_THRESHOLD_G > 1.0


def test_coast_threshold_above_free_fall():
    """Coast reads ~1 g from gravity, so the burnout threshold must sit
    above it or COAST fires during the burn."""
    assert COAST_THRESHOLD_G > 1.0


def test_apogee_velocity_window_is_tight():
    """Too wide and apogee fires during coast."""
    assert 0 < APOGEE_VEL_MPS <= 3.0


# --- accel_magnitude ---------------------------------------------------------


def test_accel_magnitude_of_rest_is_one_g():
    assert abs(accel_magnitude((0.0, 0.0, 9.80665)) - 1.0) < 1e-6


def test_accel_magnitude_is_orientation_independent():
    """The payload sits in the tube at an unknown roll angle. Magnitude
    must not depend on which axis gravity lands on."""
    g = 9.80665
    for vec in ((g, 0, 0), (0, g, 0), (0, 0, g), (0, 0, -g)):
        assert abs(accel_magnitude(vec) - 1.0) < 1e-6


def test_accel_magnitude_combines_axes():
    g = 9.80665
    assert abs(accel_magnitude((3 * g, 4 * g, 0.0)) - 5.0) < 1e-6


def test_accel_magnitude_of_free_fall_is_zero():
    assert accel_magnitude((0.0, 0.0, 0.0)) == 0.0


# --- Robustness --------------------------------------------------------------


def test_zero_pressure_does_not_raise():
    """A failed barometer read can return garbage. It must not crash."""
    fs = FlightState()
    fs.set_ground_reference(GROUND_HPA)
    fs.update_altitude(0.0, 100)
    assert fs.alt_m == 0.0


def test_repeated_timestamps_do_not_divide_by_zero():
    fs = FlightState()
    fs.set_ground_reference(GROUND_HPA)
    fs.update_altitude(alt_to_pressure(10.0), 1000)
    fs.update_altitude(alt_to_pressure(20.0), 1000)  # same t


@pytest.mark.parametrize("dt_ms", [20, 40, 100])
def test_apogee_detection_survives_rate_changes(dt_ms):
    """If the loop runs slower than intended, apogee must still be found."""
    fs = FlightState()
    history = [s for _, s in run_profile(fs, 120.0, dt_ms=dt_ms)]
    assert State.APOGEE in history