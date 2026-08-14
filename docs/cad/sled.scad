SHOW_GHOSTS = false;
SHOW_COUPONS = false;
SHOW_SLED = true;
$fn = 48;

// ---- Spine ----
SPINE_Z       = 80.0;
SPINE_Y       = 38.0;   // set by battery pocket (30 + 2*4mm walls); bore limit
SPINE_THICK   = 3.0;

// ---- Inserts (M2.5 x D3.0 x L4.0) ----
INSERT_BORE_D = 3.2; // DIA of pilot hole
INSERT_DEPTH  = 5.0;
INSERT_WALL_MIN_THICKNESS = 1.6;
INSERT_WALL_MAX_THICKNESS = 2.0;
BOLT_HOLE_D = 3.2;      // clearance for M3 alignment bolts (or 2.6 for M2.5)

// ---- Posts (sized to load) ----
POST_OD_HEAVY = ((INSERT_BORE_D / 2) + INSERT_WALL_MAX_THICKNESS)*2;  // Feather, battery bar
POST_OD_LIGHT = ((INSERT_BORE_D / 2) + INSERT_WALL_MIN_THICKNESS)*2;  // GPS, IMU, BMP
POST_H_STD    = max(4.5, INSERT_DEPTH);
POST_H_GPS    = max(5.6, INSERT_DEPTH);;  // clears 4.12mm coin holder + 1.5mm

// ---- Footprints ----
PCB_RADIUS    = 2.5;    // Corner radius of the PCB
PCB_THICKNESS = 2.0;    // Thickness of PCB
PCB_HOLE_D    = 2.5;    // Diameter of pass-thru holes on PCB

// ---- Feather ----
FEATHER_Z      = 50.8;  // 2.00"
FEATHER_Y      = 22.9;  // 0.90"
FEATHER_HOLE_Z = 45.7;  // 1.80"
FEATHER_HOLE_Y = 17.8;  // 0.70"

// ---- GPS ----
GPS_Z      = 25.4;      // 1.00"
GPS_Y      = 25.4;      // 1.00"
GPS_HOLE_Z = 20.3;      // 0.80"
GPS_HOLE_Y = 20.3;      // 0.80"

// ---- IMU (LSM6DSOX+LIS3MDL) ----
IMU_Z = 25.4;           // 1.00"
IMU_Y = 17.8;           // 0.70"
IMU_HOLE_Z = 20.3;      // 0.80"
IMU_HOLE_Y = 12.7;      // 0.50"

// ---- BMP580 ----
// NOTE: This footprint only has 2 mounting holes
BMP_Z          = 25.4;  // 1.00"
BMP_Y          = 17.8;  // 0.70"
BMP_HOLE_Z     = 20.3;  // 0.80"
BMP_HOLE_INSET = 2.5;   // 0.10" (from edge... see "Calculated")

// ---- Battery LP503035 ----
BATT_Z           = 35.0;// Z-lengh of battery
BATT_Y           = 30.0;// Y-width of battery
BATT_THICK       = 5.0; // Thickness of battery
BATT_POCKET_WALL = 4.0; // do NOT increase (bore limit)

// ---- On/Off switch ----
// https://www.amazon.com/dp/B0GSVGVN2N
ONOFF_Z = 3.8;
ONOFF_Y = 8.7;
ONOFF_X = 3.9;
ONOFF_SWITCH_HOLE_Z = 3.0;
ONOFF_SWITCH_HOLE_Y = 1.75;
ONOFF_SWITCH_HOLE_X = 4.0;
ONOFF_TOP_THICK = 0.6;
ONOFF_SIDE_THICK = 1.6;

// --- Payload Coins
COIN_D              = 44.5;  // 
COIN_THICK          = 1.5;
COIN_SPINE_THICK    = 4.5;
COIN_BATT_CUTOUT_D  = 29.2;
COIN_CUTOUT_D       = 22.2;

// ---- Calculated ----
BMP_HOLE_Y  = BMP_Y - 2 * BMP_HOLE_INSET;      // Calculated position of BMP sensor hole
SENSOR_GAP  = (SPINE_Y - IMU_Y - BMP_Y)/3;     // Gap between the sensors on Side B
FEATHER_GAP = (SPINE_Z - FEATHER_Z - GPS_Z)/3; // Gab between the Feather, GPS and edges on Side A

module rounded_rect(w, h, thick, r, $fn=32) {
  rotate([90, 0, 90]) {
    linear_extrude(height=thick)
    minkowski() {
      square([w - 2*r, h - 2*r], center=true);
      circle(r=r);
    }
  }
}

module rounded_cube(size, radius) {
    width = size[0];
    depth = size[1];
    height = size[2];
    
    hull() {
        // Place cylinders at each corner
        translate([radius, radius, 0])
            cylinder(r=radius, h=height, $fn=32);
        translate([width-radius, radius, 0])
            cylinder(r=radius, h=height, $fn=32);
        translate([radius, depth-radius, 0])
            cylinder(r=radius, h=height, $fn=32);
        translate([width-radius, depth-radius, 0])
            cylinder(r=radius, h=height, $fn=32);
    }
}

module ghost(y, z, col) {
  color(col,0.35)
    rounded_rect(y, z, PCB_THICKNESS, PCB_RADIUS);
}

//ONOFF_Z = 100;
//ONOFF_Y = 8.7;
//ONOFF_X = 3.9;
//ONOFF_SWITCH_HOLE_Z = 3.0;
//ONOFF_SWITCH_HOLE_Y = 1.75;
//ONOFF_SWITCH_HOLE_X = 4.0;
//ONOFF_TOP_THICK = 0.6;
//ONOFF_SIDE_THICK = 1.6;

module spine() {
  difference() {
    union() {
      translate([-1, -(ONOFF_Y + 2 * ONOFF_SIDE_THICK)/2, SPINE_Z+COIN_THICK-ONOFF_SWITCH_HOLE_Z-ONOFF_TOP_THICK-.2]) {
        difference() {
          translate([0, 0, 0]) {cube([
            ONOFF_X + 2 * ONOFF_SIDE_THICK,
            (ONOFF_Y + 2 * ONOFF_SIDE_THICK),
            ONOFF_Z
          ]);}
        }
      }
      translate([-SPINE_THICK/2, -SPINE_Y/2, 0]) {
        // Spine
        cube([SPINE_THICK, SPINE_Y, SPINE_Z]);
        translate([-BATT_POCKET_WALL-3, 0, SPINE_Z]) {
          rotate([-90, 0, 0]) {
            difference() {
              // Battery holder material
              cube([BATT_POCKET_WALL+5, SPINE_Y-2, BATT_Z + 3]);
              translate([1, (SPINE_Y - BATT_Y - 2)/2-2, -5]) {
                // Battery pocket
                union() {
                  rounded_cube([BATT_THICK + 2, BATT_Y + 2, BATT_Z + 20], PCB_RADIUS);
                  translate([-2, 2.5, 0]) {
                    rounded_cube([BATT_THICK, BATT_Z - 8, BATT_Z + 20], PCB_RADIUS);
                  }
                }
              }
            }
          }
        }
      }
    }
    // Mass cutout #1
    cutout_gap = 7;
    translate([-COIN_D/2, 0, SPINE_Z - COIN_THICK - (SPINE_Y - (2*cutout_gap))/2 - cutout_gap]) {
      rounded_rect(
        (SPINE_Y - (2*cutout_gap)),
        (SPINE_Y - (2*cutout_gap) - 5),
        COIN_D,
        5
      );
    }

    // Mass cutout #2
    translate([-COIN_D/2, 0, SPINE_Z/2 - 19]) {
      rounded_rect(
        8,
        8,
        COIN_D,
        1.5
      );
    }
    
    // Mass cutout #3
    translate([-COIN_D/2, 0, SPINE_Z/2]) {
      rounded_rect(
        (SPINE_Y - (2*cutout_gap)),
        5,
        COIN_D,
        1.5
      );
    }
    
    // ONOFF Cutout
    translate([-1, -(ONOFF_Y + 2 * ONOFF_SIDE_THICK)/2, SPINE_Z+COIN_THICK-ONOFF_SWITCH_HOLE_Z-ONOFF_TOP_THICK-1.2]) {
      translate([ONOFF_SIDE_THICK ,ONOFF_SIDE_THICK, -ONOFF_TOP_THICK]) {
        union() {
          cube([ONOFF_X + 5, ONOFF_Y, ONOFF_Z]);
          translate([(ONOFF_X - ONOFF_SWITCH_HOLE_Y)/2, (ONOFF_Y - ONOFF_SWITCH_HOLE_X)/2, ONOFF_Z]) {
            cube([ONOFF_SWITCH_HOLE_Y, ONOFF_SWITCH_HOLE_X, ONOFF_SWITCH_HOLE_Z]);
          }
        }
      }
    }
  }
  // Spine coin
  translate([0, 0, SPINE_Z/2 + 4]) {
    difference() {
      cylinder(r = COIN_D/2, h = COIN_SPINE_THICK);
      translate([0, -(FEATHER_Y+5)/2, -FEATHER_Z/2]) {
        cube([FEATHER_Y, FEATHER_Y+5, FEATHER_Z]);
      }
      translate([10, -COIN_D/2, -FEATHER_Z/2]) {
        cube([COIN_D, COIN_D, FEATHER_Z]);
      }
    }
  }
}

module post(x, y, h, od) {
  rotate([0, -90, 0]) {
    translate([x, y, 0]) {
      difference() {
        cylinder(h = h, r1 = od/2, r2 = 1.3 * (od/2));
        translate([0, 0, INSERT_DEPTH-h]) {
          cylinder(h = INSERT_DEPTH, r = INSERT_BORE_D/2);
        }
      }
      if (SHOW_GHOSTS) {
        translate([0, 0, h - INSERT_DEPTH]) {
          color("Magenta",0.35)
            cylinder(h = INSERT_DEPTH, r = INSERT_BORE_D/2);
        }
      }
    }
  }
}

module post_feather(x, y) {
  post(x, y, POST_H_STD, POST_OD_HEAVY);
}

module post_gps(x, y) {
  post(x, y, POST_H_GPS, POST_OD_LIGHT);
}

module post_std(x, y) {
  post(x, y, POST_H_STD, POST_OD_LIGHT);
}

// --- Feather Mounts + Ghost
// Posts flush with X=0, YZ at midpoint
module feather_posts() {
  translate([POST_H_STD, 0, 0]) {
    if (SHOW_GHOSTS) {
      feather_ghost();
    }
    translate([0, -FEATHER_HOLE_Y/2, FEATHER_HOLE_Z/2]) {
      post_feather(0, 0);
      post_feather(0, FEATHER_HOLE_Y);
      post_feather(-FEATHER_HOLE_Z, 0);
      post_feather(-FEATHER_HOLE_Z, FEATHER_HOLE_Y);
    }
  }
}

module feather_ghost() {
  difference() {
    ghost(FEATHER_Y, FEATHER_Z, "GreenYellow");
    translate([0, -FEATHER_HOLE_Y/2, -FEATHER_HOLE_Z/2]) {
      rotate([0, 90, 0]) {
        translate([0, 0, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([0, FEATHER_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([-FEATHER_HOLE_Z, 0, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([-FEATHER_HOLE_Z, FEATHER_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
      }
    }
  }
}

// --- GPS Mounts + Ghost
// Posts flush with X=0, YZ at midpoint
module gps_posts() {
  translate([POST_H_GPS, 0, 0]) {
    if (SHOW_GHOSTS) {
      gps_ghost();
    }
    translate([0, -GPS_HOLE_Y/2, GPS_HOLE_Z/2]) {
      post_gps(0, 0);
      post_gps(0, GPS_HOLE_Y);
      post_gps(-GPS_HOLE_Z, 0);
      post_gps(-GPS_HOLE_Z, GPS_HOLE_Y);
    }
  }
}

module gps_ghost() {
  difference() {
    ghost(GPS_Y, GPS_Z, "DeepSkyBlue");
    translate([0, -GPS_HOLE_Y/2, -GPS_HOLE_Z/2]) {
      rotate([0, 90, 0]) {
        translate([0, 0, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([0, GPS_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([-GPS_HOLE_Z, 0, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([-GPS_HOLE_Z, GPS_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
      }
    }
  }
}

// --- IMU Mounts + Ghost
// Posts flush with X=0, YZ at midpoint
module imu_posts() {
  translate([-POST_H_STD, 0, 0]) {
    rotate([0, 0, 180]) {
      if (SHOW_GHOSTS) {
        imu_ghost();
      }
      translate([0, -IMU_HOLE_Y/2, IMU_HOLE_Z/2]) {
        post_std(0, 0);
        post_std(0, IMU_HOLE_Y);
        post_std(-IMU_HOLE_Z, 0);
        post_std(-IMU_HOLE_Z, IMU_HOLE_Y);
      }
    }
  }
}

module imu_ghost() {
  difference() {
    ghost(IMU_Y, IMU_Z, "OrangeRed");
    translate([0, -IMU_HOLE_Y/2, -IMU_HOLE_Z/2]) {
      rotate([0, 90, 0]) {
        translate([0, 0, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([0, IMU_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([-IMU_HOLE_Z, 0, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([-IMU_HOLE_Z, IMU_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
      }
    }
  }
}

// --- BMP580 Mounts + Ghost
module bmp_posts() {
  translate([-POST_H_STD, 0, 0]) {
    rotate([0, 0, 180]) {
      if (SHOW_GHOSTS) {
        bmp_ghost();
      }
      translate([0, -BMP_HOLE_Y/2, BMP_HOLE_Z/2]) {
        post_std(0, BMP_HOLE_Y);
        post_std(-BMP_HOLE_Z, BMP_HOLE_Y);
      }
    }
  }
}

module bmp_ghost() {
  difference() {
    ghost(BMP_Y, BMP_Z, "SaddleBrown");
    translate([0, -BMP_HOLE_Y/2, -BMP_HOLE_Z/2]) {
      rotate([0, 90, 0]) {
        translate([0, BMP_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([-BMP_HOLE_Z, BMP_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
      }
    }
  }
}

module sled() {
  spine();
  // Side A
  translate([SPINE_THICK/2, 0, SPINE_Z - (FEATHER_Z/2) - FEATHER_GAP]) { feather_posts(); }
  translate([SPINE_THICK/2, 0, (GPS_Z/2) + FEATHER_GAP]) { gps_posts(); }

  // Side B
  translate([-SPINE_THICK/2, (SPINE_Y/2)-(IMU_Y/2)-SENSOR_GAP, (3*IMU_Z/4) + 3*SENSOR_GAP]) { imu_posts(); }
  translate([-SPINE_THICK/2, -(SPINE_Y/2)+(BMP_Y/2)+SENSOR_GAP, (3*BMP_Z/4) + 3*SENSOR_GAP]) { bmp_posts(); }  
}

module alignment_holes() {
  translate([-SPINE_THICK, 0, 0]) {
    rotate([0, 90, 0]) {
//      translate([-SPINE_Z+15, 0, 0])     {cylinder(h = SPINE_THICK*2, d = BOLT_HOLE_D);}
//      translate([-SPINE_Z+30, 0, 0])     {cylinder(h = SPINE_THICK*2, d = BOLT_HOLE_D);}
//      translate([-SPINE_Z+49, 15.5, 0])  {cylinder(h = SPINE_THICK*2, d = BOLT_HOLE_D);}
//      translate([-SPINE_Z+49, -15.5, 0]) {cylinder(h = SPINE_THICK*2, d = BOLT_HOLE_D);}
//      translate([-SPINE_Z+58, 0, 0])     {cylinder(h = SPINE_THICK*2, d = BOLT_HOLE_D);}
    }
  }
}

module payload_top_coin() {
  difference() {
    cylinder(h = COIN_THICK, r = COIN_D/2);
//    translate([ONOFF_Z+5, -(ONOFF_Y +4)/2, 0]) {cube([COIN_D/2-4-ONOFF_Z, ONOFF_Y+4, COIN_THICK]);}
      translate([0, 0, 0]) {
        difference() {
          cylinder(h = COIN_THICK, r = COIN_BATT_CUTOUT_D/2);
          translate([-COIN_BATT_CUTOUT_D, -COIN_BATT_CUTOUT_D/2, 0]) {cube([COIN_BATT_CUTOUT_D, COIN_BATT_CUTOUT_D, COIN_THICK]);}
          translate([0, -COIN_BATT_CUTOUT_D/2-(ONOFF_Y)/2-1.6, 0]) {cube([COIN_BATT_CUTOUT_D, COIN_BATT_CUTOUT_D/2, COIN_THICK]);}
          translate([0, COIN_BATT_CUTOUT_D/2-(ONOFF_Y)/2-4.3, 0]) {cube([COIN_BATT_CUTOUT_D, COIN_BATT_CUTOUT_D/2, COIN_THICK]);}
        }
      }
  }
}

module payload_bottom_coin() {
  JST_H = 7.0;
  JST_W = 8.0;
  difference() {
    cylinder(h = COIN_THICK, r = COIN_D/2);
    translate([-(COIN_D/2), 0, 0]) {cylinder(h = COIN_THICK, r = COIN_CUTOUT_D/2);}
    translate([(COIN_D/2), 0, 0]) {cylinder(h = COIN_THICK, r = COIN_CUTOUT_D/2);}
  }
}

// then wherever you build the full sled before splitting:
module sled_with_holes() {
  difference() {
    sled();
    alignment_holes();
  }
  translate([0, 0, -COIN_THICK]) {payload_bottom_coin();}
  translate([0, 0, SPINE_Z]) {payload_top_coin();}
}

module side_b(){
  difference() {
    sled_with_holes();
    translate([0, -(COIN_D + 5)/2, -7]) {cube([COIN_D, COIN_D + 5, SPINE_Z + 2*COIN_THICK +10 ]);}
  }
}

module side_a(){
  difference() {
    sled_with_holes();
    translate([-COIN_D, -(COIN_D + 5)/2, -7]) {cube([COIN_D, COIN_D + 5, SPINE_Z + 2*COIN_THICK + 10]);}
  }
}

if (SHOW_SLED) {
  translate ([0, 0, SPINE_THICK/2]) {
    sled_with_holes();
    rotate([0, 90, 0]) {
//      translate([0, SPINE_Y/2 + 10, 0]) {rotate ([0, 0, 180]) { side_a(); }}
//      translate ([0, -SPINE_Y/2-10, 0]) { side_b(); }
    }
  }
}

if (SHOW_COUPONS) {
  coupon_base =  2 * POST_OD_HEAVY + 15;
  translate([-coupon_base, -2-coupon_base/2, SPINE_THICK]) {
    rotate([0, -90, 0]) {
      gps_posts();
      translate([-SPINE_THICK, 0, 0]) {
        rounded_rect(coupon_base, coupon_base, SPINE_THICK, PCB_RADIUS, $fn=32);
      }
    }
  }
}