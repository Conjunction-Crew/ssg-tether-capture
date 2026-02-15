function dydt = tether_dynamics_v4(t, y, P)
% =====================================================================
% TETHER_DYNAMICS_V4 - Orbital tether dynamics ODE function
% =====================================================================
% Computes derivatives for the tether system including:
% - Axial spring-damper forces
% - Contact forces with target
% - External control forces (via spiral_controller_v4)
% - HCW (Hill-Clohessy-Wiltshire) orbital mechanics
% - Torsional forces
%
% INPUTS:
%   t - Current time [s]
%   y - State vector [pos; vel; theta; omega]
%   P - Parameters structure
%
% OUTPUT:
%   dydt - State derivatives [vel; acc; omega; alpha]
% =====================================================================

N = P.N; m = P.m; k = P.k; b = P.b; L0 = P.L0;
k_tors = P.k_tors; c_tors = P.c_tors;
target = P.target; target_r = P.target_r; k_contact = P.k_contact;
epsL = 1e-12;

% ---------- Orbital parameters ----------
mu_E = 3.986004418e14;  % Earth gravitational parameter [m^3/s^2]
Re_E = 6378.137e3;      % Earth radius [m]
alt = 600e3;            % Orbit altitude [m]
n = sqrt(mu_E / (Re_E + alt)^3);  % Mean motion [rad/s]

% ---------- Unpack state ----------
pos = reshape(y(1:2*N), [N,2]);
vel = reshape(y(2*N+1:4*N), [N,2]);
m_i = m*ones(N,1);

% ---------- Axial spring-damper forces ----------
acc_ax = zeros(N,2);
for i = 2:N
    r = pos(i,:) - pos(i-1,:);
    Li = max(norm(r), epsL);
    u = r / Li;
    dv = vel(i,:) - vel(i-1,:);
    ext = Li - L0;
    
    % Spring force (only in tension)
    Fs = (ext > 0) * (k * ext * u);
    
    % Damping force (only when extending)
    Fd = [0 0];
    if (ext > 0) && (dot(r,dv)/Li > 0)
        Fd = b * ((1-L0/Li)*dv + (L0/Li^3)*dot(r,dv)*r);
    end
    
    % Apply forces to adjacent nodes
    F = Fs + Fd;
    acc_ax(i-1,:) = acc_ax(i-1,:) + F/m;
    acc_ax(i,:)   = acc_ax(i,:) - F/m;
end

% ---------- Contact forces with target ----------
acc_contact = zeros(N,2);
for i = 1:N
    rci = pos(i,:) - target;
    d = norm(rci);
    if d < target_r
        % Repulsive force when inside target sphere
        acc_contact(i,:) = k_contact*(target_r-d)*rci/max(d,epsL)/m;
    end
end

% ---------- External control forces (v4 controller) ----------
[Fext, ~, ~, ~] = spiral_controller_v4(t, pos, vel, P);
acc_ext = Fext ./ m_i;

% ---------- HCW (Hill-Clohessy-Wiltshire) accelerations ----------
acc_hcw = zeros(N,2);
acc_hcw(:,1) = 3*n^2*pos(:,1) + 2*n*vel(:,2);
acc_hcw(:,2) = -2*n*vel(:,1);

% ---------- Total acceleration ----------
acc = acc_ax + acc_contact + acc_ext + acc_hcw;

% ---------- Add torsional effects ----------
acc = acc + compute_torsion_acc(pos, vel, acc, N, L0, k_tors, c_tors, m);

% ---------- Pack derivatives ----------
dydt = [vel(:); acc(:); zeros(2*N,1)];

end

%% ========================================================================
%  TORSION ACCELERATION
%  ========================================================================
function acc_tor = compute_torsion_acc(pos, vel, acc, N, L0, k_tors, c_tors, m)
% Compute accelerations due to torsional stiffness and damping

epsL = 1e-12;
acc_tor = zeros(N,2);

if N < 3 || k_tors == 0, return; end

% ---------- Link geometry ----------
e = pos(2:end,:) - pos(1:end-1,:);
L = max(sqrt(sum(e.^2,2)), epsL);
ux = e(:,1)./L; uy = e(:,2)./L;

% ---------- Link angles ----------
theta_link = atan2(uy, ux);
wrapPi = @(a) atan2(sin(a), cos(a));

% ---------- Link angular velocities for damping ----------
dv = vel(2:end,:) - vel(1:end-1,:);
omega_link = (-uy.*dv(:,1) + ux.*dv(:,2)) ./ L;

% Rotation matrix (90 degrees CCW)
J = [0 -1; 1 0];

% ---------- Loop over interior nodes ----------
for i = 2:N-1
    % Only apply torsion if both adjacent links are in tension
    if L(i-1) <= L0 || L(i) <= L0, continue; end
    
    % Bending angle (deviation from straight line)
    phi = wrapPi(theta_link(i) - theta_link(i-1) - pi);
    if abs(phi) > pi/2, phi = phi - sign(phi)*pi; end
    
    % Relative angular velocity
    theta_dot = omega_link(i) - omega_link(i-1);
    
    % Torsional torque (spring + damper)
    tau = -k_tors*phi - c_tors*theta_dot;
    if abs(tau) < epsL, continue; end
    
    % Compute geometric gradients
    eL = pos(i,:) - pos(i-1,:); Lm = norm(eL);
    eR = pos(i+1,:) - pos(i,:); Lp = norm(eR);
    
    g_im1 = (J * eL.').' / (Lm*Lm);
    g_ip1 = -(J * eR.').' / (Lp*Lp);
    g_i = -g_im1 - g_ip1;
    
    % Apply torque-induced accelerations
    acc_tor(i-1,:) = acc_tor(i-1,:) + tau*g_im1/m;
    acc_tor(i,:)   = acc_tor(i,:) + tau*g_i/m;
    acc_tor(i+1,:) = acc_tor(i+1,:) + tau*g_ip1/m;
end

end
