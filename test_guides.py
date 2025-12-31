class ReachyMini:
    """Reachy Mini class for controlling a simulated or real Reachy Mini robot."""

    def __init__(
        self,
        robot_name: str = "reachy_mini",

        localhost_only: bool = True,

        spawn_daemon: bool = False,

        use_sim: bool = False,
    ):
        """Initialize the Reachy Mini controller.

        Args:
            robot_name: Name of the robot
            localhost_only: If True, will only connect to localhost daemons

            spawn_daemon: If True, will spawn a daemon to control the robot

            use_sim: If True and spawn_daemon is True, will spawn a simulated robot
        """
        self.robot_name = robot_name
        self.localhost_only = localhost_only

        if spawn_daemon:
            self._start_daemon(use_sim)

        self._connect()

    def _start_daemon(self, use_sim: bool):
        """Start the robot daemon."""
        if use_sim:
            print("Starting simulated robot...")

        else:
            print("Starting real robot...")

        # Initialize hardware
        self._init_hardware()

    def move_to(self, x: float, y: float, z: float):
        """Move the robot to a position."""
        if not self._is_connected():
            raise RuntimeError("Robot not connected")

        # Validate coordinates
        if x < 0 or y < 0 or z < 0:
            raise ValueError("Coordinates must be positive")

        # Execute movement
        self._execute_move(x, y, z)
